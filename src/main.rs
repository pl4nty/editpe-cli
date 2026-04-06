use clap::{ArgAction, Parser};
use editpe::{
    Image, ResourceData, ResourceDirectory, ResourceEntry, ResourceEntryName, ResourceTable,
    VersionInfo, VersionStringTable,
    constants::*,
    types::VersionU32,
};

/// Command line tool to edit resources of exe files.
///
/// API-compatible with https://github.com/electron/rcedit.
#[derive(Debug, Parser)]
#[command(
    name = "editpe-cli",
    about = "Command line tool to edit resources of exe file",
    arg_required_else_help = true,
    disable_help_flag = true
)]
struct Cli {
    /// Path to the exe or dll to edit
    filename: String,

    /// Set a version string (e.g. --set-version-string ProductName "My App")
    #[arg(long, num_args = 2, value_names = ["KEY", "VALUE"], action = ArgAction::Append)]
    set_version_string: Vec<String>,

    /// Set the file version (e.g. 1.2.3.4)
    #[arg(long, value_name = "VERSION", action = ArgAction::Append)]
    set_file_version: Vec<String>,

    /// Set the product version (e.g. 1.2.3.4)
    #[arg(long, value_name = "VERSION", action = ArgAction::Append)]
    set_product_version: Vec<String>,

    /// Set the icon from a file
    #[arg(long, value_name = "PATH", action = ArgAction::Append)]
    set_icon: Vec<String>,

    /// Set a string table resource by numeric id
    #[arg(long, num_args = 2, value_names = ["ID", "VALUE"], action = ArgAction::Append)]
    set_resource_string: Vec<String>,

    /// Set the requested execution level in the manifest
    /// (asInvoker | highestAvailable | requireAdministrator)
    #[arg(long, value_name = "LEVEL", action = ArgAction::Append)]
    set_requested_execution_level: Vec<String>,

    /// Set the application manifest from a file
    #[arg(long, value_name = "PATH", action = ArgAction::Append)]
    application_manifest: Vec<String>,

    /// Get a version string and print it to stdout
    #[arg(long, value_name = "KEY", action = ArgAction::Append)]
    get_version_string: Vec<String>,

    /// Get a string table resource by numeric id and print it to stdout
    #[arg(long, value_name = "ID", action = ArgAction::Append)]
    get_resource_string: Vec<String>,

    /// Print help information
    #[arg(short, long, action = ArgAction::Help)]
    help: Option<bool>,
}

fn parse_version(s: &str) -> Result<(u16, u16, u16, u16), String> {
    let parts: Vec<&str> = s.split('.').collect();
    let parse_part = |idx: usize| -> Result<u16, String> {
        match parts.get(idx) {
            None => Ok(0),
            Some(p) => p
                .parse::<u16>()
                .map_err(|_| format!("invalid version component '{}' in '{}'", p, s)),
        }
    };
    Ok((parse_part(0)?, parse_part(1)?, parse_part(2)?, parse_part(3)?))
}

fn get_resource_string(resources: &ResourceDirectory, id: u32) -> Option<String> {
    let block_id = id / 16 + 1;
    let position = (id % 16) as usize;

    let type_table = match resources.root().get(ResourceEntryName::ID(RT_STRING as u32)) {
        Some(ResourceEntry::Table(t)) => t,
        _ => return None,
    };
    let block_table = match type_table.get(ResourceEntryName::ID(block_id)) {
        Some(ResourceEntry::Table(t)) => t,
        _ => return None,
    };

    let keys = block_table.entries();
    let lang_key = keys.first().copied()?.clone();
    let data = match block_table.get(lang_key) {
        Some(ResourceEntry::Data(d)) => d.data().to_vec(),
        _ => return None,
    };

    let mut offset = 0usize;
    for i in 0..16 {
        if offset + 2 > data.len() {
            return None;
        }
        let len = u16::from_le_bytes([data[offset], data[offset + 1]]) as usize;
        offset += 2;
        if i == position {
            if len == 0 || offset + len * 2 > data.len() {
                return None;
            }
            let chars: Vec<u16> = (0..len)
                .map(|j| u16::from_le_bytes([data[offset + j * 2], data[offset + j * 2 + 1]]))
                .collect();
            return String::from_utf16(&chars).ok();
        }
        offset += len * 2;
    }
    None
}

fn set_resource_string(resources: &mut ResourceDirectory, id: u32, value: &str) {
    let block_id = id / 16 + 1;
    let position = (id % 16) as usize;
    let type_name = ResourceEntryName::ID(RT_STRING as u32);
    let block_name = ResourceEntryName::ID(block_id);

    // Ensure RT_STRING type table exists
    if resources.root().get(&type_name).is_none() {
        resources
            .root_mut()
            .insert(type_name.clone(), ResourceEntry::Table(ResourceTable::default()));
    }

    // Ensure block table exists inside type table
    {
        let type_table = match resources.root_mut().get_mut(&type_name) {
            Some(ResourceEntry::Table(t)) => t,
            _ => return,
        };
        if type_table.get(&block_name).is_none() {
            type_table.insert(block_name.clone(), ResourceEntry::Table(ResourceTable::default()));
        }
    }

    // Read current data for the block (or empty if no language entry exists yet)
    let existing_data: Vec<u8> = {
        let type_table = match resources.root_mut().get_mut(&type_name) {
            Some(ResourceEntry::Table(t)) => t,
            _ => return,
        };
        let block_table = match type_table.get_mut(&block_name) {
            Some(ResourceEntry::Table(t)) => t,
            _ => return,
        };

        if block_table.entries().is_empty() {
            block_table.insert(
                ResourceEntryName::default(),
                ResourceEntry::Data(ResourceData::default()),
            );
            Vec::new()
        } else {
            let key = block_table.entries().first().copied().unwrap().clone();
            match block_table.get(&key) {
                Some(ResourceEntry::Data(d)) => d.data().to_vec(),
                _ => return,
            }
        }
    };

    // Parse the 16-string block, padding with empty strings as needed
    let mut strings: Vec<Vec<u16>> = Vec::with_capacity(16);
    let mut offset = 0usize;
    for _ in 0..16 {
        if offset + 2 > existing_data.len() {
            strings.push(Vec::new());
            continue;
        }
        let len = u16::from_le_bytes([existing_data[offset], existing_data[offset + 1]]) as usize;
        offset += 2;
        let chars = if len > 0 && offset + len * 2 <= existing_data.len() {
            (0..len)
                .map(|j| {
                    u16::from_le_bytes([
                        existing_data[offset + j * 2],
                        existing_data[offset + j * 2 + 1],
                    ])
                })
                .collect::<Vec<u16>>()
        } else {
            Vec::new()
        };
        offset += len * 2;
        strings.push(chars);
    }
    while strings.len() < 16 {
        strings.push(Vec::new());
    }

    strings[position] = value.encode_utf16().collect();

    // Rebuild binary data for the block
    let mut new_data: Vec<u8> = Vec::new();
    for chars in &strings {
        new_data.extend_from_slice(&(chars.len() as u16).to_le_bytes());
        for &c in chars {
            new_data.extend_from_slice(&c.to_le_bytes());
        }
    }

    // Write back
    {
        let type_table = match resources.root_mut().get_mut(&type_name) {
            Some(ResourceEntry::Table(t)) => t,
            _ => return,
        };
        let block_table = match type_table.get_mut(&block_name) {
            Some(ResourceEntry::Table(t)) => t,
            _ => return,
        };
        let key = block_table.entries().first().copied().unwrap().clone();
        if let Some(ResourceEntry::Data(d)) = block_table.get_mut(&key) {
            d.set_data(new_data);
        }
    }
}

/// Modify the `level` attribute of `<requestedExecutionLevel>` in an XML manifest string.
/// If the element or attribute is not found, a minimal manifest containing it is returned.
fn set_requested_execution_level(manifest: &str, level: &str) -> String {
    if let Some(elem_pos) = manifest.find("requestedExecutionLevel") {
        let after_elem = &manifest[elem_pos..];
        if let Some(attr_rel) = after_elem.find("level=") {
            let attr_start = elem_pos + attr_rel + 6; // skip past `level=`
            if let Some(quote) = manifest[attr_start..].chars().next() {
                if quote == '"' || quote == '\'' {
                    if let Some(end_rel) = manifest[attr_start + 1..].find(quote) {
                        let end_pos = attr_start + 1 + end_rel;
                        return format!(
                            "{}{}{}{}",
                            &manifest[..attr_start + 1],
                            level,
                            quote,
                            &manifest[end_pos + 1..]
                        );
                    }
                }
            }
        }
    }

    // No existing manifest or element: create a minimal manifest with the requested level
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
         <assembly xmlns=\"urn:schemas-microsoft-com:asm.v1\" manifestVersion=\"1.0\">\n  \
           <trustInfo xmlns=\"urn:schemas-microsoft-com:asm.v3\">\n    \
             <security>\n      \
               <requestedPrivileges>\n        \
                 <requestedExecutionLevel level=\"{}\" uiAccess=\"false\"/>\n      \
               </requestedPrivileges>\n    \
             </security>\n  \
           </trustInfo>\n\
         </assembly>",
        level
    )
}

/// Build an ordered list of operations from the parsed CLI arguments.
///
/// clap collects multi-use flags independently, losing cross-flag ordering.  
/// We recover the original order by re-scanning `std::env::args()` and mapping
/// each occurrence back to the pre-parsed values.
enum Operation {
    SetVersionString(String, String),
    SetFileVersion(String),
    SetProductVersion(String),
    SetIcon(String),
    SetResourceString(u32, String),
    SetRequestedExecutionLevel(String),
    SetApplicationManifest(String),
    GetVersionString(String),
    GetResourceString(u32),
}

fn ordered_operations(cli: &Cli) -> Result<Vec<Operation>, String> {
    // Iterators that dispense pre-validated values in encounter order
    let mut svs = cli.set_version_string.chunks(2);
    let mut sfv = cli.set_file_version.iter();
    let mut spv = cli.set_product_version.iter();
    let mut si = cli.set_icon.iter();
    let mut srs = cli.set_resource_string.chunks(2);
    let mut srel = cli.set_requested_execution_level.iter();
    let mut am = cli.application_manifest.iter();
    let mut gvs = cli.get_version_string.iter();
    let mut grs = cli.get_resource_string.iter();

    let mut ops = Vec::new();
    let mut raw = std::env::args().skip(2); // skip binary name + filename

    while let Some(flag) = raw.next() {
        match flag.as_str() {
            "--set-version-string" => {
                // consume the two positional arguments that clap already ate
                raw.next();
                raw.next();
                let pair = svs.next().unwrap();
                ops.push(Operation::SetVersionString(pair[0].clone(), pair[1].clone()));
            }
            "--set-file-version" => {
                raw.next();
                ops.push(Operation::SetFileVersion(sfv.next().unwrap().clone()));
            }
            "--set-product-version" => {
                raw.next();
                ops.push(Operation::SetProductVersion(spv.next().unwrap().clone()));
            }
            "--set-icon" => {
                raw.next();
                ops.push(Operation::SetIcon(si.next().unwrap().clone()));
            }
            "--set-resource-string" => {
                raw.next();
                raw.next();
                let pair = srs.next().unwrap();
                let id: u32 = pair[0]
                    .parse()
                    .map_err(|_| format!("invalid resource string id '{}'", pair[0]))?;
                ops.push(Operation::SetResourceString(id, pair[1].clone()));
            }
            "--set-requested-execution-level" => {
                raw.next();
                ops.push(Operation::SetRequestedExecutionLevel(srel.next().unwrap().clone()));
            }
            "--application-manifest" => {
                raw.next();
                ops.push(Operation::SetApplicationManifest(am.next().unwrap().clone()));
            }
            "--get-version-string" => {
                raw.next();
                ops.push(Operation::GetVersionString(gvs.next().unwrap().clone()));
            }
            "--get-resource-string" => {
                raw.next();
                let raw_id = grs.next().unwrap();
                let id: u32 = raw_id
                    .parse()
                    .map_err(|_| format!("invalid resource string id '{}'", raw_id))?;
                ops.push(Operation::GetResourceString(id));
            }
            _ => {} // filename or unknown flags already rejected by clap
        }
    }

    Ok(ops)
}

fn main() {
    let cli = Cli::parse();

    let operations = match ordered_operations(&cli) {
        Ok(ops) => ops,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    if operations.is_empty() {
        eprintln!("error: no operations specified");
        std::process::exit(1);
    }

    let filename = &cli.filename;

    let image_data = match std::fs::read(filename) {
        Ok(data) => data,
        Err(e) => {
            eprintln!("error: failed to read '{}': {}", filename, e);
            std::process::exit(1);
        }
    };

    let mut image = match Image::parse(&image_data[..]) {
        Ok(img) => img,
        Err(e) => {
            eprintln!("error: failed to parse '{}': {}", filename, e);
            std::process::exit(1);
        }
    };

    let mut resources = image.resource_directory().cloned().unwrap_or_default();
    let mut modified = false;

    for op in &operations {
        match op {
            Operation::SetVersionString(key, value) => {
                let mut version_info = match resources.get_version_info() {
                    Ok(Some(vi)) => vi,
                    Ok(None) => VersionInfo::default(),
                    Err(e) => {
                        eprintln!("error: failed to read version info: {}", e);
                        std::process::exit(1);
                    }
                };
                if version_info.strings.is_empty() {
                    version_info.strings.push(VersionStringTable {
                        key:     format!("{:04X}{:04X}", LANGUAGE_ID_EN_US, CODE_PAGE_ID_EN_US),
                        strings: Default::default(),
                    });
                }
                version_info.strings[0].strings.insert(key.clone(), value.clone());
                if let Err(e) = resources.set_version_info(&version_info) {
                    eprintln!("error: failed to set version string: {}", e);
                    std::process::exit(1);
                }
                modified = true;
            }
            Operation::SetFileVersion(version_str) => {
                let (major, minor, patch, build) = match parse_version(version_str) {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!("error: {}", e);
                        std::process::exit(1);
                    }
                };
                let mut version_info = match resources.get_version_info() {
                    Ok(Some(vi)) => vi,
                    Ok(None) => VersionInfo::default(),
                    Err(e) => {
                        eprintln!("error: failed to read version info: {}", e);
                        std::process::exit(1);
                    }
                };
                version_info.info.file_version = VersionU32 {
                    major: ((major as u32) << 16) | (minor as u32),
                    minor: ((patch as u32) << 16) | (build as u32),
                };
                if let Err(e) = resources.set_version_info(&version_info) {
                    eprintln!("error: failed to set file version: {}", e);
                    std::process::exit(1);
                }
                modified = true;
            }
            Operation::SetProductVersion(version_str) => {
                let (major, minor, patch, build) = match parse_version(version_str) {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!("error: {}", e);
                        std::process::exit(1);
                    }
                };
                let mut version_info = match resources.get_version_info() {
                    Ok(Some(vi)) => vi,
                    Ok(None) => VersionInfo::default(),
                    Err(e) => {
                        eprintln!("error: failed to read version info: {}", e);
                        std::process::exit(1);
                    }
                };
                version_info.info.product_version = VersionU32 {
                    major: ((major as u32) << 16) | (minor as u32),
                    minor: ((patch as u32) << 16) | (build as u32),
                };
                if let Err(e) = resources.set_version_info(&version_info) {
                    eprintln!("error: failed to set product version: {}", e);
                    std::process::exit(1);
                }
                modified = true;
            }
            Operation::SetIcon(path) => {
                if let Err(e) = resources.set_main_icon_file(path) {
                    eprintln!("error: failed to set icon from '{}': {}", path, e);
                    std::process::exit(1);
                }
                modified = true;
            }
            Operation::SetResourceString(id, value) => {
                set_resource_string(&mut resources, *id, value);
                modified = true;
            }
            Operation::SetRequestedExecutionLevel(level) => {
                let existing = match resources.get_manifest() {
                    Ok(Some(m)) => m,
                    Ok(None) => String::new(),
                    Err(e) => {
                        eprintln!("error: failed to read manifest: {}", e);
                        std::process::exit(1);
                    }
                };
                let new_manifest = set_requested_execution_level(&existing, level);
                if let Err(e) = resources.set_manifest(&new_manifest) {
                    eprintln!("error: failed to set manifest: {}", e);
                    std::process::exit(1);
                }
                modified = true;
            }
            Operation::SetApplicationManifest(path) => {
                let manifest = match std::fs::read_to_string(path) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("error: failed to read manifest file '{}': {}", path, e);
                        std::process::exit(1);
                    }
                };
                if let Err(e) = resources.set_manifest(&manifest) {
                    eprintln!("error: failed to set manifest: {}", e);
                    std::process::exit(1);
                }
                modified = true;
            }
            Operation::GetVersionString(key) => {
                let version_info = match resources.get_version_info() {
                    Ok(Some(vi)) => vi,
                    Ok(None) => {
                        eprintln!("error: no version info present in '{}'", filename);
                        std::process::exit(1);
                    }
                    Err(e) => {
                        eprintln!("error: failed to read version info: {}", e);
                        std::process::exit(1);
                    }
                };
                let value = version_info
                    .strings
                    .iter()
                    .find_map(|table| table.strings.get(key.as_str()))
                    .cloned();
                match value {
                    Some(v) => println!("{}", v),
                    None => {
                        eprintln!("error: version string '{}' not found", key);
                        std::process::exit(1);
                    }
                }
            }
            Operation::GetResourceString(id) => match get_resource_string(&resources, *id) {
                Some(s) => println!("{}", s),
                None => {
                    eprintln!("error: resource string {} not found", id);
                    std::process::exit(1);
                }
            },
        }
    }

    if modified {
        if let Err(e) = image.set_resource_directory(resources) {
            eprintln!("error: failed to update resource directory: {}", e);
            std::process::exit(1);
        }
        if let Err(e) = image.write_file(filename) {
            eprintln!("error: failed to write '{}': {}", filename, e);
            std::process::exit(1);
        }
    }
}
