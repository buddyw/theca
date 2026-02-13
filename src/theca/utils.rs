use std::fs::{read_dir, File};
use std::io::{Write, Read, stdout};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::env::{var};
use std::iter::repeat;
use std::time::UNIX_EPOCH;

use crossterm::{
    style::{Attribute, SetAttribute},
    execute,
    tty::IsTty,
};

// tempfile imports
use tempfile::Builder; // replacement for TempDir

use std::io::stdin;

// theca imports
use crate::{specific_fail, specific_fail_str};
use crate::errors::{Result, Error, ErrorKind};
use crate::lineformat::LineFormat;
use crate::profile::{DATEFMT_SHORT, Profile, ProfileFlags}; // Import ProfileFlags
use crate::item::{Item, Status};

pub use libc::{STDIN_FILENO, STDOUT_FILENO, STDERR_FILENO};

pub fn istty(fd: i32) -> bool {
    // crossterm provides IsTty trait for stdout/stdin
    match fd {
        STDOUT_FILENO => stdout().is_tty(),
        _ => false // simplified
    }
}

pub fn termsize() -> usize {
    if let Ok((cols, _rows)) = crossterm::terminal::size() {
        cols as usize
    } else {
        0
    }
}

pub fn extract_status(status_str: Option<String>) -> Result<Option<Status>> {
    match status_str.as_deref() {
        Some("started") | Some("Started") => Ok(Some(Status::Started)),
        Some("urgent") | Some("Urgent") => Ok(Some(Status::Urgent)),
        Some("done") | Some("Done") => Ok(Some(Status::Urgent)),
        Some("blank") | Some("Blank") | Some("none") => Ok(Some(Status::Blank)),
        None => Ok(None),
        Some(_) => specific_fail_str!("Invalid status (started,urgent,done, or none)"),
    }
}

pub fn drop_to_editor(contents: &str) -> Result<String> {
    // setup temporary file
    let tmpfile = Builder::new()
        .prefix("theca")
        .suffix(".txt")
        .rand_bytes(5)
        .tempfile()?;
            
    let tmppath = tmpfile.path().to_owned();
    
    // Write contents
    {
        let mut file = File::create(&tmppath)?;
        file.write_all(contents.as_bytes())?;
    }

    let editor = var("VISUAL").or_else(|_| var("EDITOR"))
        .unwrap_or_else(|_| "nano".to_string()); // Default to nano if not set rather than fail? Or fail. Originals failed.
        
    // lets start `editor` and edit the file at `tmppath`
    let mut editor_command = Command::new(&editor);
    editor_command.arg(&tmppath.display().to_string());
    editor_command.stdin(Stdio::inherit());
    editor_command.stdout(Stdio::inherit());
    editor_command.stderr(Stdio::inherit());
    
    let mut editor_proc = editor_command.spawn().map_err(|e| Error {
        kind: ErrorKind::Generic,
        desc: format!("Failed to start editor '{}': {}", editor, e),
        detail: None,
    })?;

    if editor_proc.wait().is_ok() {
        // finished editing, read file
        let mut file = File::open(&tmppath)?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        Ok(content)
    } else {
        specific_fail_str!("The editor process failed.")
    }
}

pub fn get_password() -> Result<String> {
    print!("Key: ");
    stdout().flush()?;
    let password = rpassword::read_password().map_err(|e| Error {
        kind: ErrorKind::Generic,
        desc: format!("Failed to read password: {}", e),
        detail: None,
    })?;
    Ok(password)
}

pub fn get_new_password() -> Result<String> {
    loop {
        print!("New Key: ");
        stdout().flush()?;
        let p1 = rpassword::read_password().map_err(|e| Error {
            kind: ErrorKind::Generic,
            desc: format!("Failed to read password: {}", e),
            detail: None,
        })?;
        
        print!("Confirm Key: ");
        stdout().flush()?;
        let p2 = rpassword::read_password().map_err(|e| Error {
            kind: ErrorKind::Generic, // Reusing generic error
            desc: format!("Failed to read password: {}", e),
            detail: None,
        })?;

        if p1 == p2 {
            if p1.is_empty() {
                 println!("Key cannot be empty.");
                 continue;
            }
            return Ok(p1);
        }
        println!("Keys do not match. Please try again.");
    }
}

pub fn get_yn_input(message: &str) -> Result<bool> {
    print!("{}", message);
    stdout().flush()?;
    
    let stdin = stdin();
    let yes = vec!["y", "Y", "yes", "YES", "Yes"];
    let no = vec!["n", "N", "no", "NO", "No"];
    
    loop {
        print!("[y/n]# ");
        stdout().flush()?;
        let mut input = String::new();
        stdin.read_line(&mut input)?;
        let input = input.trim();
        if yes.contains(&input) {
            return Ok(true);
        } else if no.contains(&input) {
            return Ok(false);
        };
        println!("invalid input.");
    }
}

pub fn pretty_line(bold: &str, plain: &str, tty: bool) -> Result<()> {
    let mut stdout = stdout();
    if tty {
        execute!(stdout, SetAttribute(Attribute::Bold))?;
    }
    print!("{}", bold);
    if tty {
         execute!(stdout, SetAttribute(Attribute::Reset))?;
    }
    print!("{}", plain);
    Ok(())
}

pub fn format_field(value: &str, width: usize, truncate: bool) -> String {
    if value.len() > width && width > 3 && truncate {
        format!("{: <1$.1$}...", value, width - 3)
    } else {
        format!("{: <1$.1$}", value, width)
    }
}

pub fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | ' ' => c,
            _ => '_',
        })
        .collect::<String>()
        .trim()
        .to_string()
}

fn print_header(line_format: &LineFormat) -> Result<()> {
    let mut stdout = stdout();
    let column_seperator: String = repeat(' ')
                                       .take(line_format.colsep)
                                       .collect();
    let header_seperator: String = repeat('-')
                                       .take(line_format.line_width())
                                       .collect();
    let tty = istty(STDOUT_FILENO);
    let status = if line_format.status_width == 0 {
        "".to_string()
    } else {
        format_field(&"status".to_string(), line_format.status_width, false) + &*column_seperator
    };
    
    if tty {
        execute!(stdout, SetAttribute(Attribute::Bold))?;
    }
    print!(
                "{1}{0}{2}{0}{3}{4}\n{5}\n",
                column_seperator,
                format_field(&"id".to_string(), line_format.id_width, false),
                format_field(&"title".to_string(), line_format.title_width, false),
                status,
                format_field(&"last touched".to_string(),
                             line_format.touched_width,
                             false),
                header_seperator);
    if tty {
        execute!(stdout, SetAttribute(Attribute::Reset))?;
    }
    Ok(())
}

pub fn sorted_print(notes: &mut Vec<Item>,
                    limit: usize,
                    flags: ProfileFlags,
                    status: Option<Status>)
                    -> Result<()> {
    let condensed = flags.condensed;
    let yaml = flags.yaml;
    let datesort = flags.datesort;
    let reverse = flags.reverse;
    let search_body = flags.search_body;

    if let Some(status) = status {
        notes.retain(|n| n.status == status);
    }
    let limit = if limit != 0 && limit < notes.len() {
        limit
    } else {
        notes.len()
    };
    
    if datesort {
        notes.sort_by(|a, b| {
             let a_tm = parse_last_touched(&a.last_touched).unwrap_or(chrono::Local::now());
             let b_tm = parse_last_touched(&b.last_touched).unwrap_or(chrono::Local::now());
             a_tm.cmp(&b_tm)
        });
    }

    if reverse {
        notes.reverse();
    }

    if yaml {
        println!("{}", serde_yaml::to_string(&notes[0..limit].to_vec()).unwrap())
    } else {
        let line_format = LineFormat::new(&notes[0..limit], condensed, search_body)?;
        if !condensed && !yaml {
            print_header(&line_format)?;
        }
        for n in notes[0..limit].iter() {
            n.print(&line_format, search_body)?;
        }
    };

    Ok(())
}

pub fn find_profile_folder(profile_folder: &Option<String>) -> Result<PathBuf> {
    if let Some(pf) = profile_folder {
        Ok(PathBuf::from(pf))
    } else {
        match dirs::home_dir() {
            Some(p) => {
                let default_path = p.join(".theca");
                if default_path.is_file() {
                    let mut file = File::open(&default_path)?;
                    let mut contents = String::new();
                    file.read_to_string(&mut contents)?;
                    let trimmed = contents.trim();
                    if trimmed.is_empty() {
                         return specific_fail_str!("~/.theca is a file but is empty. It should contain a path to the profile directory.");
                    }
                    Ok(PathBuf::from(trimmed))
                } else {
                    Ok(default_path)
                }
            },
            None => specific_fail_str!("failed to find your home directory"),
        }
    }
}

pub fn parse_last_touched(lt: &str) -> Result<chrono::DateTime<chrono::Local>> {
    lt.parse::<chrono::DateTime<chrono::Local>>().map_err(Error::from)
}

pub fn localize_last_touched_string(lt: &str) -> Result<String> {
    let t = parse_last_touched(lt)?;
    Ok(t.format(DATEFMT_SHORT).to_string())
}

pub fn validate_profile_from_path(profile_path: &PathBuf) -> (bool, bool) {
    // return (is_a_profile, encrypted(?))
    if let Some(ext) = profile_path.extension() {
        if ext == "yaml" || ext == "json" { 
             match File::open(profile_path) {
                Ok(mut f) => {
                    let mut contents_buf: Vec<u8> = vec![];
                    if f.read_to_end(&mut contents_buf).is_err() {
                        return (false, false);
                    }
                    
                    if let Ok(s) = String::from_utf8(contents_buf.clone()) {
                        // try parsed
                        if serde_yaml::from_str::<Profile>(&s).is_ok() {
                            return (true, false);
                        }
                        // try json (legacy) handled by yaml parser often, but strict match might fail?
                        // serde_yaml 0.9 can parse JSON. So we might be good with just from_str (yaml).
                        // But let's keep it simple.
                        // Actually I removed serde_json from Cargo.toml?
                        // If so I CANNOT USE IT.
                        // I will remove explicit serde_json call.
                    }
                    // If we are here, it might be encrypted
                    return (true, true);
                }
                Err(_) => return (false, false),
             }
        }
    }
    (false, false)
}

pub fn path_to_profile_name(profile_path: &PathBuf) -> Result<String> {
    let just_f = profile_path.file_stem().unwrap();
    Ok(just_f.to_str().unwrap().to_string())
}

pub fn profiles_in_folder(folder: &Path) -> Result<()> {
    if folder.is_dir() {
        println!("# profiles in {}", folder.display());
        
        // Check for special 'default' profile in root
        let root_profile = folder.join("profile.yaml");
        let is_root_prof = validate_profile_from_path(&root_profile);
        if is_root_prof.0 {
            let mut msg = "default".to_string();
            if is_root_prof.1 {
                msg = format!("{} [encrypted]", msg);
            }
            println!("    {}", msg);
        }

        for entry in read_dir(folder)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                let profile_yaml = path.join("profile.yaml");
                let is_prof = validate_profile_from_path(&profile_yaml);
                if is_prof.0 {
                    let mut msg = path_to_profile_name(&path)?; // path is the dir, stem is dir name
                    if is_prof.1 {
                        msg = format!("{} [encrypted]", msg);
                    }
                    println!("    {}", msg);
                }
            }
        }
    }
    Ok(())
}

pub fn profile_fingerprint<P: AsRef<Path>>(path: P) -> Result<u64> {
    let path = path.as_ref();
    let metadata = path.metadata()?;
    let modified = metadata.modified()?;
    let since_epoch = modified.duration_since(UNIX_EPOCH)?;
    Ok(since_epoch.as_secs())
}
