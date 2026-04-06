use clap::{ArgAction, Parser};
use editpe::{
    Image, ResourceData, ResourceDirectory, ResourceEntry, ResourceEntryName, ResourceTable,
    VersionInfo, VersionStringTable, constants::*, types::VersionU32,
};

/// Command line tool to edit resources of exe files.
///
/// API-compatible with https://github.com/electron/rcedit.
#[derive(Debug, Parser)]
#[command(
    name = "editpe",
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

    /// Set an RCDATA resource by id (numeric or named) from a file
    #[arg(long, num_args = 2, value_names = ["ID", "PATH"], action = ArgAction::Append)]
    set_rcdata: Vec<String>,

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

fn die(msg: impl std::fmt::Display) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(1);
}

fn parse_version(s: &str) -> Result<(u16, u16, u16, u16), String> {
    let parts: Vec<&str> = s.split('.').collect();
    let p = |i: usize| {
        parts.get(i).map_or(Ok(0), |p| {
            p.parse::<u16>()
                .map_err(|_| format!("invalid version component '{p}' in '{s}'"))
        })
    };
    Ok((p(0)?, p(1)?, p(2)?, p(3)?))
}

fn make_version(major: u16, minor: u16, patch: u16, build: u16) -> VersionU32 {
    VersionU32 {
        major: ((major as u32) << 16) | minor as u32,
        minor: ((patch as u32) << 16) | build as u32,
    }
}

fn load_version_info(resources: &ResourceDirectory) -> VersionInfo {
    match resources.get_version_info() {
        Ok(Some(vi)) => vi,
        Ok(None) => VersionInfo::default(),
        Err(e) => die(format!("failed to read version info: {e}")),
    }
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
            block_table
                .insert(ResourceEntryName::default(), ResourceEntry::Data(ResourceData::default()));
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
                 <requestedExecutionLevel level=\"{level}\" uiAccess=\"false\"/>\n      \
               </requestedPrivileges>\n    \
             </security>\n  \
           </trustInfo>\n\
         </assembly>"
    )
}

fn main() {
    let cli = Cli::parse();
    let filename = &cli.filename;

    if [
        cli.get_version_string.as_slice(),
        cli.get_resource_string.as_slice(),
        cli.set_version_string.as_slice(),
        cli.set_file_version.as_slice(),
        cli.set_product_version.as_slice(),
        cli.set_icon.as_slice(),
        cli.set_resource_string.as_slice(),
        cli.set_requested_execution_level.as_slice(),
        cli.application_manifest.as_slice(),
        cli.set_rcdata.as_slice(),
    ]
    .iter()
    .all(|s| s.is_empty())
    {
        die("no operations specified");
    }

    let image_data = std::fs::read(filename)
        .unwrap_or_else(|e| die(format!("failed to read '{filename}': {e}")));
    let mut image = Image::parse(&image_data)
        .unwrap_or_else(|e| die(format!("failed to parse '{filename}': {e}")));
    let mut resources = image.resource_directory().cloned().unwrap_or_default();
    let mut modified = false;

    for key in &cli.get_version_string {
        let vi = resources
            .get_version_info()
            .unwrap_or_else(|e| die(format!("failed to read version info: {e}")))
            .unwrap_or_else(|| die(format!("no version info present in '{filename}'")));
        println!(
            "{}",
            vi.strings
                .iter()
                .find_map(|t| t.strings.get(key.as_str()))
                .cloned()
                .unwrap_or_else(|| die(format!("version string '{key}' not found")))
        );
    }

    for raw_id in &cli.get_resource_string {
        let id: u32 = raw_id
            .parse()
            .unwrap_or_else(|_| die(format!("invalid resource string id '{raw_id}'")));
        println!(
            "{}",
            get_resource_string(&resources, id)
                .unwrap_or_else(|| die(format!("resource string {id} not found")))
        );
    }

    for chunk in cli.set_version_string.chunks(2) {
        let mut vi = load_version_info(&resources);
        if vi.strings.is_empty() {
            vi.strings.push(VersionStringTable {
                key:     format!("{:04X}{:04X}", LANGUAGE_ID_EN_US, CODE_PAGE_ID_EN_US),
                strings: Default::default(),
            });
        }
        vi.strings[0].strings.insert(chunk[0].clone(), chunk[1].clone());
        resources
            .set_version_info(&vi)
            .unwrap_or_else(|e| die(format!("failed to set version string: {e}")));
        modified = true;
    }

    for v in &cli.set_file_version {
        let (major, minor, patch, build) = parse_version(v).unwrap_or_else(|e| die(e));
        let mut vi = load_version_info(&resources);
        vi.info.file_version = make_version(major, minor, patch, build);
        resources
            .set_version_info(&vi)
            .unwrap_or_else(|e| die(format!("failed to set file version: {e}")));
        modified = true;
    }

    for v in &cli.set_product_version {
        let (major, minor, patch, build) = parse_version(v).unwrap_or_else(|e| die(e));
        let mut vi = load_version_info(&resources);
        vi.info.product_version = make_version(major, minor, patch, build);
        resources
            .set_version_info(&vi)
            .unwrap_or_else(|e| die(format!("failed to set product version: {e}")));
        modified = true;
    }

    for path in &cli.set_icon {
        resources
            .set_main_icon_file(path)
            .unwrap_or_else(|e| die(format!("failed to set icon from '{path}': {e}")));
        modified = true;
    }

    for chunk in cli.set_resource_string.chunks(2) {
        let id_str = &chunk[0];
        let id: u32 = id_str
            .parse()
            .unwrap_or_else(|_| die(format!("invalid resource string id '{id_str}'")));
        set_resource_string(&mut resources, id, &chunk[1]);
        modified = true;
    }

    for level in &cli.set_requested_execution_level {
        let existing = resources
            .get_manifest()
            .unwrap_or_else(|e| die(format!("failed to read manifest: {e}")))
            .unwrap_or_default();
        resources
            .set_manifest(&set_requested_execution_level(&existing, level))
            .unwrap_or_else(|e| die(format!("failed to set manifest: {e}")));
        modified = true;
    }

    for path in &cli.application_manifest {
        let manifest = std::fs::read_to_string(path)
            .unwrap_or_else(|e| die(format!("failed to read manifest file '{path}': {e}")));
        resources
            .set_manifest(&manifest)
            .unwrap_or_else(|e| die(format!("failed to set manifest: {e}")));
        modified = true;
    }

    for chunk in cli.set_rcdata.chunks(2) {
        let id_str = &chunk[0];
        let path = &chunk[1];
        let id_name = match id_str.parse::<u32>() {
            Ok(n) => ResourceEntryName::ID(n),
            Err(_) => ResourceEntryName::from_string(id_str),
        };
        let data = std::fs::read(path)
            .unwrap_or_else(|e| die(format!("failed to read rcdata file '{path}': {e}")));
        let type_name = ResourceEntryName::ID(RT_RCDATA as u32);

        if resources.root().get(&type_name).is_none() {
            resources
                .root_mut()
                .insert(type_name.clone(), ResourceEntry::Table(ResourceTable::default()));
        }
        let type_table = match resources.root_mut().get_mut(&type_name) {
            Some(ResourceEntry::Table(t)) => t,
            _ => die("rcdata type entry is not a table"),
        };
        let mut inner = match type_table.get(&id_name) {
            Some(ResourceEntry::Table(t)) => t.clone(),
            _ => ResourceTable::default(),
        };
        let mut entry = ResourceData::default();
        entry.set_data(data);
        inner.insert(ResourceEntryName::default(), ResourceEntry::Data(entry));
        type_table.insert(id_name, ResourceEntry::Table(inner));
        modified = true;
    }

    if modified {
        image
            .set_resource_directory(resources)
            .unwrap_or_else(|e| die(format!("failed to update resource directory: {e}")));
        image
            .write_file(filename)
            .unwrap_or_else(|e| die(format!("failed to write '{filename}': {e}")));
    }
}
