// std lib imports
use std::io::{stdin, Read, Write};
use std::fs::{File, create_dir};
use std::path::Path;
// use std::path::{Path, PathBuf};

use serde::{Serialize, Deserialize};
use base64::{Engine as _, engine::general_purpose};

// random things
use regex::Regex;

// theca imports
use crate::utils::istty;
use crate::utils::{drop_to_editor, pretty_line, get_yn_input, sorted_print, localize_last_touched_string,
            parse_last_touched, find_profile_folder, profile_fingerprint};
use crate::{specific_fail, specific_fail_str};
use crate::errors::Result;

// Use the new crypt module
use crate::crypt::{encrypt, decrypt};
use crate::item::{Status, Item};

pub use libc::{STDIN_FILENO, STDOUT_FILENO, STDERR_FILENO};

/// datetime formating string
pub static DATEFMT: &'static str = "%F %T %z";
/// short datetime formating string for printing
pub static DATEFMT_SHORT: &'static str = "%F %T";

/// Main container of a theca profile file
#[derive(Deserialize, Serialize, Clone)]
pub struct Profile {
    pub encrypted: bool,
    pub notes: Vec<Item>,
}

pub struct ProfileFlags {
    pub condensed: bool,
    pub datesort: bool,
    pub editor: bool,
    pub encrypted: bool,
    pub yaml: bool,
    pub regex: bool,
    pub reverse: bool,
    pub search_body: bool,
    pub yes: bool,
}

impl Default for ProfileFlags {
    fn default() -> Self {
        ProfileFlags {
            condensed: false,
            datesort: false,
            editor: false,
            encrypted: false,
            yaml: false,
            regex: false,
            reverse: false,
            search_body: false,
            yes: false,
        }
    }
}

impl Profile {
    fn from_scratch(profile_folder: &Option<String>, encrypted: bool, yes: bool) -> Result<(Profile, u64)> {
        let profile_base_path = find_profile_folder(profile_folder)?;
        // if the folder doesn't exist, make it yo!
        if !profile_base_path.exists() {
            if !yes {
                let message = format!("{} doesn't exist, would you like to create it?\n",
                                      profile_base_path.display());
                if !get_yn_input(&message)? {
                    return specific_fail_str!("ok bye ♥");
                }
            }
            create_dir(&profile_base_path)?;
        }
        Ok((Profile {
            encrypted: encrypted,
            notes: vec![],
        },
            0u64))
    }

    fn from_existing_profile(profile_name: &str,
                             profile_folder: &Option<String>,
                             key: Option<&String>,
                             encrypted: bool)
                             -> Result<(Profile, u64)> {
        // set profile folder
        let profile_base = find_profile_folder(profile_folder)?;
        
        let (profile_dir, profile_path) = if profile_name == "default" {
            (profile_base.clone(), profile_base.join("profile.yaml"))
        } else {
            let dir = profile_base.join(profile_name);
            let path = dir.join("profile.yaml");
            (dir, path)
        };

        // attempt to read profile
        if profile_path.is_file() {
            let mut file = File::open(&profile_path)?;
            let mut contents_buf = vec![];
            file.read_to_end(&mut contents_buf)?;
            let contents = if encrypted {
                let key_val = if let Some(k) = key {
                    k.clone()
                } else {
                    crate::utils::get_password()?
                };

                // Decrypt
                // 1. Read as UTF-8 string (Base64)
                let b64_str = String::from_utf8(contents_buf)
                   .map_err(|_| "Failed to read encrypted file as UTF-8/Base64. Is it a legacy binary?")?;
                // 2. Decode Base64
                let encrypted_bytes = general_purpose::STANDARD.decode(b64_str.trim())
                   .map_err(|e| {
                       if b64_str.contains("encrypted:") {
                           format!("Profile on disk appears to be plaintext (found 'encrypted:' key), but --encrypted was specified. Try without --encrypted. Original error: {}", e)
                       } else {
                           format!("Base64 decode error: {}", e)
                       }
                   })?;
                // 3. Decrypt
                match decrypt(&encrypted_bytes, &key_val) {
                   Ok(decrypted) => String::from_utf8(decrypted)?,
                   Err(_) => return specific_fail_str!("Decryption failed. Wrong key?"),
                }
            } else {
                String::from_utf8(contents_buf)?
            };
            
            let decoded: Profile = match serde_yaml::from_str(&contents) {
                Ok(s) => s,
                Err(e) => {
                    return specific_fail!(format!("invalid YAML in {}: {}", profile_path.display(), e))
                }
            };
            let fingerprint = profile_fingerprint(&profile_path)?;
            Ok((decoded, fingerprint))
        } else if profile_dir.exists() && profile_name != "default" && !profile_path.exists() {
             // Directory exists but no profile.yaml? (only for named profiles)
             specific_fail!(format!("Profile directory {} exists but contains no profile.yaml.", profile_dir.display()))
        } else {
             // Fallback: Check for legacy .yaml in base folder?
             // User requested specific change "instead of creating <profile_name>.yaml".
             // We can provide a helpful error if legacy file exists.
             let base = find_profile_folder(profile_folder)?;
             let legacy_yaml = base.join(format!("{}.yaml", profile_name));
             if legacy_yaml.exists() {
                 return specific_fail!(format!("Found legacy profile at {}. Please move it to {}/profile.yaml", legacy_yaml.display(), profile_dir.display()));
             }
             
             let legacy_json = base.join(format!("{}.json", profile_name));
             if legacy_json.exists() {
                 return specific_fail!(format!("Found legacy JSON profile at {}. Please migrate.", legacy_json.display()));
             }

            specific_fail!(format!("{} does not exist.", profile_path.display()))
        }
    }

    /// setup a Profile struct
    pub fn new(profile_name: &str,
               profile_folder: &Option<String>,
               key: Option<&String>,
               new_profile: bool,
               encrypted: bool,
               yes: bool)
               -> Result<(Profile, u64)> {
        if new_profile {
            Profile::from_scratch(profile_folder, encrypted, yes)
        } else {
            Profile::from_existing_profile(profile_name, profile_folder, key, encrypted)
        }
    }

    /// remove all notes from the profile
    pub fn clear(&mut self, yes: bool) -> Result<()> {
        if !yes {
            let message = "are you sure you want to delete all the notes in this profile?\n";
            if !get_yn_input(&message)? {
                return specific_fail_str!("ok bye ♥");
            }
        }
        self.notes.truncate(0);
        Ok(())
    }

    fn sync_markdown_files(&self, profile_dir: &Path) -> Result<()> {
        let mut kept_files = std::collections::HashSet::new();

        for note in &self.notes {
            let sanitized_title = crate::utils::sanitize_filename(&note.title);
            let filename = format!("{}-{}.md", note.id, sanitized_title);
            let file_path = profile_dir.join(&filename);

            let mut content = String::from("---\n");
            content.push_str(&format!("id: {}\n", note.id));
            content.push_str(&format!("title: {}\n", note.title));
            content.push_str(&format!("status: {}\n", note.status));
            content.push_str(&format!("last_touched: {}\n", note.last_touched));
            content.push_str("---\n");
            content.push_str(&note.body);

            // Fail gracefully
            if let Ok(mut f) = File::create(&file_path) {
                let _ = f.write_all(content.as_bytes());
            }
            kept_files.insert(filename);
        }

        // Cleanup orphans
        if let Ok(entries) = std::fs::read_dir(profile_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("md") {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if !kept_files.contains(name) {
                            let _ = std::fs::remove_file(path);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn delete_markdown_files(&self, profile_dir: &Path) -> Result<()> {
        if let Ok(entries) = std::fs::read_dir(profile_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("md") {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
        Ok(())
    }

    /// save the profile back to file (either plaintext or encrypted)
    pub fn save_to_file(&mut self, 
                        profile_name: &str, 
                        profile_folder: &Option<String>, 
                        key: Option<&String>, 
                        new_profile: bool, 
                        encrypting: bool,
                        yes: bool,
                        fingerprint: &u64) -> Result<()> {
        
        // Enforce collision rules for named profiles
        if profile_name != "default" {
            if profile_name == "profile.yaml" || profile_name.ends_with(".md") {
                return specific_fail!(format!("Invalid profile name '{}'. Profile names cannot be 'profile.yaml' or end in '.md'.", profile_name));
            }
        }

        let profile_base = find_profile_folder(profile_folder)?;
        
        let (profile_dir, profile_path) = if profile_name == "default" {
            (profile_base.clone(), profile_base.join("profile.yaml"))
        } else {
            let dir = profile_base.join(profile_name);
            let path = dir.join("profile.yaml");
            (dir, path)
        };
        
        // Create directory if it doesn't exist (only for named profiles)
        if profile_name != "default" && !profile_dir.exists() {
             std::fs::create_dir_all(&profile_dir)?;
        }

        if new_profile && profile_path.exists() && !yes {
            let message = format!("profile {} already exists would you like to overwrite it?\n",
                                  profile_path.display());
            if !get_yn_input(&message)? {
                return specific_fail_str!("ok bye ♥");
            }
        }

        if fingerprint > &0u64 {
            let new_fingerprint = profile_fingerprint(&profile_path)?;
            if &new_fingerprint != fingerprint && !yes {
                 // Simple merge conflict check
                 return specific_fail!(format!("Profile '{}' has been modified on disk. Please reload.", profile_name));
            }
        }

        // open file
        let mut file = File::create(&profile_path)?;

        // encode to buffer
        let yaml_prof = serde_yaml::to_string(&self).map_err(|e| format!("Serialization error: {}", e))?;

        // encrypt if its an encrypted profile
        let buffer = if self.encrypted {
            if let Some(k) = key {
                let encrypted_bytes = encrypt(yaml_prof.as_bytes(), k).map_err(|e| format!("Encryption error: {}", e))?;
                general_purpose::STANDARD.encode(&encrypted_bytes).into_bytes()
            } else {
                 return specific_fail_str!("Profile is encrypted but no key provided");
            }
        } else {
            yaml_prof.into_bytes()
        };

        // write buffer to file
        file.write_all(&buffer)?;

        // Handle markdown export
        if !self.encrypted {
            let _ = self.sync_markdown_files(&profile_dir);
        } else if encrypting {
            let _ = self.delete_markdown_files(&profile_dir);
        }

        Ok(())
    }

    /// transfer a note from the profile to another profile
    pub fn transfer_note(&mut self, 
                        note_id: usize, 
                        target_profile_name: &str,
                        current_profile_name: &str,
                        profile_folder: &Option<String>,
                        key: Option<&String>,
                        encrypted: bool,
                        yes: bool) -> Result<()> {
        
        if current_profile_name == target_profile_name {
            return specific_fail!(format!("cannot transfer a note from a profile to itself"));
        }

        let (mut trans_profile, trans_fingerprint) = Profile::new(
            target_profile_name,
            profile_folder,
            key,
            false, // assuming target exists or we fail? Original logic: new=cmd_new_profile from args.
            // But logic seems to imply we load target. 
            // If target needs to be created, we should probably know.
            // The original code passed `args.cmd_new_profile` which likely meant 
            // `theca new` command, so transferring likely assumes existing target unless new flag used?
            // Let's assume false for transfer target usually. 
            encrypted, // This assumes target has same encryption?? Original used args.flag_encrypted.
            yes)?;

        if let Some(pos) = self.notes.iter().position(|n| n.id == note_id) {
             let n = &self.notes[pos];
             trans_profile.add_note(&n.title,
                                    &[n.body.clone()],
                                    Some(n.status),
                                    false,
                                    false,
                                    false)?;
             
             // Save target
             trans_profile.save_to_file(target_profile_name, profile_folder, key, false, false, yes, &trans_fingerprint)?;
             
             // Remove from source
             self.notes.remove(pos);
             
             println!("transfered [{}: note {} -> {}: note {}]",
                  current_profile_name,
                  note_id,
                  target_profile_name,
                  trans_profile.notes.last().map_or(0, |n| n.id));

        } else {
            return specific_fail!(format!("Note {} not found", note_id));
        }
        
        Ok(())
    }

    /// add a item to the profile
    pub fn add_note(&mut self,
                    title: &str,
                    body: &[String],
                    status: Option<Status>,
                    use_stdin: bool,
                    use_editor: bool,
                    print_msg: bool)
                    -> Result<()> {
        let title = title.replace("\n", "").to_string();

        let body = if use_stdin {
            let mut buf = String::new();
            stdin().read_to_string(&mut buf)?;
            buf.to_owned()
        } else if !use_editor {
             if body.is_empty() {
                "".to_string()
            } else {
                body.join("\n")
            }
        } else if istty(STDOUT_FILENO) && istty(STDIN_FILENO) {
            let next_id = match self.notes.last() {
                Some(n) => n.id + 1,
                None => 1,
            };
            drop_to_editor(&"".to_string(), Some(next_id), Some(&title))?
        } else {
            "".to_string()
        };

        let new_id = match self.notes.last() {
            Some(n) => n.id,
            None => 0,
        };
        self.notes.push(Item {
            id: new_id + 1,
            title: title,
            status: status.unwrap_or(Status::Blank),
            body: body,
            last_touched: chrono::Local::now().format(DATEFMT).to_string(),
        });
        if print_msg {
            println!("note {} added", new_id + 1);
        }
        Ok(())
    }

    /// delete an item from the profile
    pub fn delete_note(&mut self, id: &[usize]) {
        for nid in id.iter() {
            let remove = self.notes
                             .iter()
                             .position(|n| &n.id == nid)
                             .map(|e| self.notes.remove(e))
                             .is_some();
            if remove {
                println!("deleted note {}", nid);
            } else {
                println!("note {} doesn't exist", nid);
            }
        }
    }

    /// edit an item in the profile
    pub fn edit_note(&mut self,
                     id: usize,
                     title: &Option<String>,
                     body: &Option<String>,
                     status: &Option<Status>,
                     use_stdin: bool,
                     flags: ProfileFlags)
                     -> Result<()> {
        
        let item_pos: usize = match self.notes.iter().position(|n| n.id == id) {
            Some(i) => i,
            None => return specific_fail!(format!("note {} doesn't exist", id)),
        };
        let use_editor = flags.editor;
        let encrypted = flags.encrypted;
        let yes = flags.yes;

        if let Some(t) = title {
            if !t.is_empty() {
                self.notes[item_pos].title = t.replace("\n", "").to_string();
            }
        }

        if let Some(s) = status {
            self.notes[item_pos].status = *s;
        }

        if let Some(b) = body {
             self.notes[item_pos].body = b.clone();
        } else if use_stdin {
             let mut buf = String::new();
             stdin().read_to_string(&mut buf)?;
             self.notes[item_pos].body = buf;
        } else if use_editor {
            if istty(STDOUT_FILENO) && istty(STDIN_FILENO) {
                if encrypted && !yes {
                    let message = format!("{0}\n\n{1}\n{2}\n\n{0}\n{3}\n",
                                          "## [WARNING] ##",
                                          "continuing will write the body of the decrypted \
                                           note to a temporary",
                                          "file, increasing the possibilty it could be \
                                           recovered later.",
                                          "Are you sure you want to continue?\n");
                    if !get_yn_input(&message)? {
                        return specific_fail_str!("ok bye ♥");
                    }
                }
                let new_body = drop_to_editor(&self.notes[item_pos].body, Some(id), Some(&self.notes[item_pos].title))?;
                if self.notes[item_pos].body != new_body {
                    self.notes[item_pos].body = new_body;
                }
            }
        }

        // update last_touched
        self.notes[item_pos].last_touched = chrono::Local::now().format(DATEFMT).to_string();
        println!("edited note {}", self.notes[item_pos].id);
        Ok(())
    }

    /// print information about the profile
    pub fn stats(&mut self, name: &str) -> Result<()> {
        let no_s = self.notes.iter().filter(|n| n.status == Status::Blank).count();
        let started_s = self.notes
                            .iter()
                            .filter(|n| n.status == Status::Started)
                            .count();
        let urgent_s = self.notes
                           .iter()
                           .filter(|n| n.status == Status::Urgent)
                           .count();
        let tty = istty(STDOUT_FILENO);
        if self.notes.is_empty() {
             pretty_line("name: ", &format!("{}\n", name), tty)?;
             pretty_line("encrypted: ", &format!("{}\n", self.encrypted), tty)?;
             pretty_line("notes: ", "0\n", tty)?;
        } else {
            let min = match self.notes
                                .iter()
                                .min_by_key(|n| match parse_last_touched(&*n.last_touched) {
                                    Ok(o) => o,
                                    Err(_) => chrono::Local::now(),
                                }) {
                Some(n) => localize_last_touched_string(&*n.last_touched)?,
                None => return specific_fail_str!("last_touched is not properly formated"),
            };
            let max = match self.notes
                                .iter()
                                .max_by_key(|n| match parse_last_touched(&*n.last_touched) {
                                    Ok(o) => o,
                                    Err(_) => chrono::Local::now(),
                                }) {
                Some(n) => localize_last_touched_string(&*n.last_touched)?,
                None => return specific_fail_str!("last_touched is not properly formated"),
            };
            pretty_line("name: ", &format!("{}\n", name), tty)?;
            pretty_line("encrypted: ", &format!("{}\n", self.encrypted), tty)?;
            pretty_line("notes: ", &format!("{}\n", self.notes.len()), tty)?;
            pretty_line("statuses: ",
                             &format!("none: {}, started: {}, urgent: {}\n",
                                      no_s,
                                      started_s,
                                      urgent_s),
                             tty)?;
            pretty_line("note ages: ",
                             &format!("oldest: {}, newest: {}\n", min, max),
                             tty)?;
        }
        Ok(())
    }

    /// print a full item
    pub fn view_note(&mut self, id: usize, yaml: bool, condensed: bool) -> Result<()> {
        let id = id;
        let note_pos = match self.notes.iter().position(|n| n.id == id) {
            Some(i) => i,
            None => return specific_fail!(format!("note {} doesn't exist", id)),
        };
        if yaml {
            println!("{}", serde_yaml::to_string(&self.notes[note_pos].clone()).unwrap());
        } else {
            let tty = istty(STDOUT_FILENO);

            if condensed {
                pretty_line("id: ", &format!("{}\n", self.notes[note_pos].id), tty)?;
                pretty_line("title: ", &format!("{}\n", self.notes[note_pos].title), tty)?;
                if self.notes[note_pos].status != Status::Blank {
                pretty_line("status: ",
                                 &format!("{}\n", self.notes[note_pos].status),
                                 tty)?;
                }
                pretty_line("last touched: ",
                             &format!("{}\n",
                            localize_last_touched_string(
                                &*self.notes[note_pos].last_touched
                            )
                        ?),
                             tty)?;
            } else {
                pretty_line("id\n--\n", &format!("{}\n\n", self.notes[note_pos].id), tty)?;
                pretty_line("title\n-----\n",
                                 &format!("{}\n\n", self.notes[note_pos].title),
                                 tty)?;
                if self.notes[note_pos].status != Status::Blank {
                    pretty_line("status\n------\n",
                                     &format!("{}\n\n", self.notes[note_pos].status),
                                     tty)?;
                }
                pretty_line("last touched\n------------\n",
                                 &format!("{}\n\n",
                                localize_last_touched_string(
                                    &*self.notes[note_pos].last_touched
                                )
                            ?),
                                 tty)?;
            };

            // body
            if !self.notes[note_pos].body.is_empty() {
                if condensed {
                    pretty_line("body: ", &format!("{}\n", self.notes[note_pos].body), tty)?;
                } else {
                    pretty_line("body\n----\n",
                                     &format!("{}\n\n", self.notes[note_pos].body),
                                     tty)?;
                };
            }
        }
        Ok(())
    }

    /// print all notes in the profile
    pub fn list_notes(&mut self,
                      limit: usize,
                      flags: ProfileFlags,
                      status: Option<Status>)
                      -> Result<()> {
        if !self.notes.is_empty() {
            sorted_print(&mut self.notes.clone(), limit, flags, status)?;
        } else if flags.yaml {
            println!("[]");
        } else {
            println!("this profile is empty");
        }
        Ok(())
    }

    /// print notes search for in the profile
    pub fn search_notes(&mut self,
                        pattern: &str,
                        limit: usize,
                        flags: ProfileFlags,
                        status: Option<Status>)
                        -> Result<()> {
        let notes: Vec<Item> = if flags.regex {
            let re = match Regex::new(&pattern[..]) {
                Ok(r) => r,
                Err(e) => return specific_fail!(format!("regex error: {}.", e)),
            };
            self.notes
                .iter()
                .filter(|n| if flags.search_body {
                    re.is_match(&*n.body)
                } else {
                    re.is_match(&*n.title)
                })
                .cloned()
                .collect()
        } else {
            self.notes
                .iter()
                .filter(|n| if flags.search_body {
                    n.body.contains(&pattern[..])
                } else {
                    n.title.contains(&pattern[..])
                })
                .cloned()
                .collect()
        };
        if !notes.is_empty() {
            sorted_print(&mut notes.clone(), limit, flags, status)?;
        } else if flags.yaml {
            println!("[]");
        } else {
            println!("nothing found");
        }
        Ok(())
    }
    /// sync notes with markdown files in the profile folder
    pub fn sync(&mut self, profile_name: &str, profile_folder: &Option<String>) -> Result<()> {
        if self.encrypted {
            return specific_fail_str!("synchronization is only supported for plaintext profiles");
        }

        let profile_base = find_profile_folder(profile_folder)?;
        
        let (profile_dir, _profile_path) = if profile_name == "default" {
            (profile_base.clone(), profile_base.join("profile.yaml"))
        } else {
            let dir = profile_base.join(profile_name);
            let path = dir.join("profile.yaml");
            (dir, path)
        };

        if !profile_dir.exists() {
            return specific_fail!(format!("profile directory {} does not exist", profile_dir.display()));
        }

        let mut new_notes_raw: Vec<(String, String)> = vec![];
        let mut seen_ids = std::collections::HashSet::new();
        // updated_notes was unused in previous version, removing it
        
        let entries = std::fs::read_dir(&profile_dir)?;
        for entry in entries.flatten() {
            let path = entry.path();
            
            // Skip directories (only matters for default profile sync, but safe everywhere)
            if path.is_dir() {
                continue;
            }

            if path.extension().and_then(|e| e.to_str()) == Some("md") {
                let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
                let mut content = String::new();
                if let Ok(mut f) = File::open(&path) {
                    let _ = f.read_to_string(&mut content);
                }

                // Check for valid frontmatter
                if content.starts_with("---\n") {
                    let parts: Vec<&str> = content.splitn(3, "---\n").collect();
                    if parts.len() == 3 {
                        let frontmatter = parts[1];
                        let body = parts[2].to_string();

                        // Parse frontmatter manually to avoid heavy dependencies if possible
                        let mut id = None;
                        let mut title = None;
                        let mut status = None;
                        // last_touched will be updated if body/title changes

                        for line in frontmatter.lines() {
                            if line.starts_with("id: ") {
                                id = line[4..].parse::<usize>().ok();
                            } else if line.starts_with("title: ") {
                                title = Some(line[7..].to_string());
                            } else if line.starts_with("status: ") {
                                let s_str = line[8..].trim();
                                status = crate::utils::extract_status(if s_str.is_empty() { None } else { Some(s_str.to_string()) }).ok().flatten();
                            }
                        }

                        if let Some(id_val) = id {
                            seen_ids.insert(id_val);
                            // Find in current notes
                            if let Some(note) = self.notes.iter_mut().find(|n| n.id == id_val) {
                                let mut changed = false;
                                if let Some(t) = title {
                                    if note.title != t {
                                        note.title = t;
                                        changed = true;
                                    }
                                }
                                if let Some(s) = status {
                                    if note.status != s {
                                        note.status = s;
                                        changed = true;
                                    }
                                }
                                if note.body != body {
                                    note.body = body;
                                    changed = true;
                                }

                                if changed {
                                    note.last_touched = chrono::Local::now().format(DATEFMT).to_string();
                                }
                            }
                            continue;
                        }
                    }
                }

                // If we reach here, it's either invalid frontmatter or new note
                new_notes_raw.push((filename, content));
                let _ = std::fs::remove_file(path);
            }
        }

        // Deletion: keep only notes that were seen in valid md files
        self.notes.retain(|n| seen_ids.contains(&n.id));

        // Add new/invalid notes
        for (filename, content) in new_notes_raw {
            let mut title = if filename.ends_with(".md") {
                filename[..filename.len() - 3].to_string()
            } else {
                filename
            };
            
            // Discard any "<N>-" at the start of the filename
            if let Some(pos) = title.find('-') {
                if title[..pos].chars().all(|c| c.is_ascii_digit()) {
                    title = title[pos+1..].to_string();
                }
            }

            // If it had frontmatter but was "invalid" (e.g. no ID), strip it for the body
            let body = if content.starts_with("---\n") {
                 let parts: Vec<&str> = content.splitn(3, "---\n").collect();
                 if parts.len() == 3 { parts[2].to_string() } else { content }
            } else {
                content
            };

            self.add_note(&title, &[body], None, false, false, false)?;
        }

        // Final sync of markdown files to disk based on new profile state
        self.sync_markdown_files(&profile_dir)?;

        println!("synchronization complete");
        Ok(())
    }
}
