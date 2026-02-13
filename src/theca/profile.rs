// std lib imports
use std::io::{stdin, Read, Write};
use std::fs::{File, create_dir};
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
        let profile_path = find_profile_folder(profile_folder)?;
        // if the folder doesn't exist, make it yo!
        if !profile_path.exists() {
            if !yes {
                let message = format!("{} doesn't exist, would you like to create it?\n",
                                      profile_path.display());
                if !get_yn_input(&message)? {
                    return specific_fail_str!("ok bye ♥");
                }
            }
            create_dir(&profile_path)?;
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
        let mut profile_path = find_profile_folder(profile_folder)?;

        // set profile name
        profile_path.push(&(profile_name.to_string() + ".yaml"));

        // attempt to read profile
        if profile_path.is_file() {
            let mut file = File::open(&profile_path)?;
            let mut contents_buf = vec![];
            file.read_to_end(&mut contents_buf)?;
            let contents = if encrypted {
                if let Some(k) = key {
                     // Decrypt
                     // 1. Read as UTF-8 string (Base64)
                     let b64_str = String::from_utf8(contents_buf)
                        .map_err(|_| "Failed to read encrypted file as UTF-8/Base64. Is it a legacy binary?")?;
                     // 2. Decode Base64
                     let encrypted_bytes = general_purpose::STANDARD.decode(b64_str.trim())
                        .map_err(|e| format!("Base64 decode error: {}", e))?;
                     // 3. Decrypt
                     match decrypt(&encrypted_bytes, k) {
                        Ok(decrypted) => String::from_utf8(decrypted)?,
                        Err(_) => return specific_fail_str!("Decryption failed. Wrong key?"),
                     }
                } else {
                    return specific_fail_str!("Profile is encrypted but no key provided");
                }
            } else {
                String::from_utf8(contents_buf)?
            };
            
            let decoded: Profile = match serde_yaml::from_str(&contents) {
                Ok(s) => s,
                Err(e) => {
                     // Fallback check for JSON for migration? (User said breaking changes ok, but helpful to know)
                     // For now strictly YAML as requested.
                    return specific_fail!(format!("invalid YAML in {}: {}", profile_path.display(), e))
                }
            };
            let fingerprint = profile_fingerprint(&profile_path)?;
            Ok((decoded, fingerprint))
        } else if profile_path.exists() {
            specific_fail!(format!("{} is not a file.", profile_path.display()))
        } else {
            // Check if json exists to warn user?
             let mut json_path = profile_path.clone();
             json_path.set_extension("json");
             if json_path.exists() {
                 return specific_fail!(format!("Found legacy JSON profile at {}. Please migrate or rename.", json_path.display()));
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

    /// save the profile back to file (either plaintext or encrypted)
    pub fn save_to_file(&mut self, 
                        profile_name: &str, 
                        profile_folder: &Option<String>, 
                        key: Option<&String>, 
                        new_profile: bool, 
                        yes: bool,
                        fingerprint: &u64) -> Result<()> {
        
        let mut profile_path = find_profile_folder(profile_folder)?;
        profile_path.push(&(profile_name.to_string() + ".yaml"));

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
        let mut file = File::create(profile_path)?;

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
             trans_profile.save_to_file(target_profile_name, profile_folder, key, false, yes, &trans_fingerprint)?;
             
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
            drop_to_editor(&"".to_string())?
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
                let new_body = drop_to_editor(&self.notes[item_pos].body)?;
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
                                     &format!("{:?}\n\n", self.notes[note_pos].status),
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
}
