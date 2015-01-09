extern crate libc;
extern crate time;
extern crate docopt;
extern crate "rustc-serialize" as rustc_serialize;
extern crate regex;
extern crate crypto;

// std lib imports...
use std::os::{getenv, homedir};
use std::io::fs::PathExtensions;
use std::io::process::{InheritFd};
use std::io::{File, Truncate, Write, Read, Open, ReadWrite, TempDir, Command, SeekSet, stdin};
use std::iter::{repeat};

// random things
use regex::{Regex};
use rustc_serialize::{Encodable, Decodable, Encoder, json};
use time::{now_utc, strftime};
use docopt::Docopt;

// crypto imports
use crypto::{symmetriccipher, buffer, aes, blockmodes};
use crypto::buffer::{ReadBuffer, WriteBuffer, BufferResult};
use crypto::pbkdf2::{pbkdf2};
use crypto::hmac::{Hmac};
use crypto::sha2::{Sha256};
use crypto::digest::Digest;
use rustc_serialize::base64::{ToBase64, FromBase64, MIME};

pub use self::libc::{
    STDIN_FILENO,
    STDOUT_FILENO,
    STDERR_FILENO
};

static VERSION:  &'static str = "0.4.0-dev";

mod c {
    extern crate libc;
    pub use self::libc::{
        c_int,
        c_ushort,
        c_ulong,
        STDOUT_FILENO
    };
    use std::mem::zeroed;
    pub struct Winsize {
        pub ws_row: c_ushort,
        pub ws_col: c_ushort
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    static TIOCGWINSZ: c_ulong = 0x5413;
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    static TIOCGWINSZ: c_ulong = 0x40087468;
    extern {
        pub fn ioctl(fd: c_int, request: c_ulong, ...) -> c_int;
    }
    pub unsafe fn dimensions() -> Winsize {
        let mut window: Winsize = zeroed();
        ioctl(STDOUT_FILENO, TIOCGWINSZ, &mut window as *mut Winsize);
        window
    }
}

fn termsize() -> Option<uint> {
    let ws = unsafe { c::dimensions() };
    if ws.ws_col == 0 || ws.ws_row == 0 {
        None
    }
    else {
        Some(ws.ws_col as uint)
    }
}

static USAGE: &'static str = "
theca - cli note taking tool

Usage:
    theca new-profile
    theca new-profile <name> [--encrypted]
    theca [options] [-c] [-l LIMIT]
    theca [options] [-c] <id>
    theca [options] [-c] view <id>
    theca [options] [-c] search <pattern>
    theca [options] [-c] search-body <pattern>
    theca [options] add <title> [--started|--urgent] [-b BODY|--editor|-]
    theca [options] edit <id> [<title>] [--started|--urgent|--none] [-b BODY|--editor|-]
    theca [options] del <id>
    theca (-h | --help)
    theca --version

Options:
    -h, --help                          Show this screen.
    -v, --version                       Show the version of theca.
    --profiles-folder PROFILEPATH       Path to folder container profile.json files.
    -p PROFILE, --profile PROFILE       Specify non-default profile.
    -c, --condensed                     Use the condensed print format.
    --encrypted                         Encrypt new profile, theca will prompt you for a key.
    -l LIMIT                            Limit listing to LIMIT items.
    --none                              No status.
    --started                           Started status.
    --urgent                            Urgent status.
    -b BODY                             Set body of the item to BODY.
    --editor                            Drop to $EDITOR to set/edit item body.
    -                                   Set body of the item to STDIN.
";

#[derive(RustcDecodable, Show)]
struct Args {
    flag_profiles_folder: Vec<String>,
    flag_p: Vec<String>,
    cmd_new_profile: bool,
    cmd_view: bool,
    cmd_search: bool,
    cmd_search_body: bool,
    cmd_add: bool,
    cmd_edit: bool,
    cmd_del: bool,
    arg_name: String,
    arg_pattern: String,
    flag_encrypted: bool,
    flag_c: bool,
    flag_l: Vec<uint>,
    arg_title: String,
    flag_started: bool,
    flag_urgent: bool,
    flag_none: bool,
    flag_b: Vec<String>,
    flag_editor: bool,
    cmd__: bool,
    arg_id: Vec<uint>,
    flag_h: bool,
    flag_v: bool
}

impl Args {
    fn check_env(&mut self) {
        match getenv("THECA_DEFAULT_PROFILE") {
            Some(val) => {
                if self.flag_p.is_empty() {
                    self.flag_p[0] = val;
                }
            },
            None => ()
        };
        match getenv("THECA_PROFILE_FOLDER") {
            Some(val) => {
                if self.flag_profiles_folder.is_empty() {
                    self.flag_profiles_folder[0] = val;
                }
            },
            None => ()
        };
    }
}

static NOSTATUS: &'static str = "";
static STARTED: &'static str = "Started";
static URGENT: &'static str = "Urgent";

#[derive(Copy)]
pub struct LineFormat {
    colsep: uint,
    id_width: uint,
    title_width: uint,
    status_width: uint,
    touched_width: uint
}

fn add_if(x: uint, y: uint, a: bool) -> uint {
    match a {
        true => x+y,
        false => x
    }
}

impl LineFormat {
    fn new(items: &Vec<ThecaItem>, args: &Args) -> LineFormat {
        // get termsize :>
        let console_width = match termsize() {
            None => panic!("Cannot retrieve terminal information"),
            Some(width) => width,
        };

        // set minimums (header length) + colsep, this should probably do some other stuff?
        let mut line_format = LineFormat {colsep: 2, id_width:0, title_width:0, status_width:0, touched_width:0};

        // find length of longest items to format line
        line_format.id_width = items.iter().max_by(|n| n.id.to_string().len()).unwrap().id.to_string().len();
        if line_format.id_width < 2 {line_format.id_width = 2;}
        line_format.title_width = items.iter().max_by(|n| add_if(n.title.len(), 4, n.body.len().ne(&0))).unwrap().title.len();
        if line_format.title_width < 5 {line_format.title_width = 5;}
        if !args.flag_c {
            line_format.status_width = items.iter().max_by(|n| n.status.len()).unwrap().status.len();
            if line_format.status_width > 0 && line_format.status_width < 7 {line_format.status_width = 7;}
        } else {
            line_format.status_width = 1;
        }
        line_format.touched_width = match args.flag_c {
            true => 10, // condensed
            false => 19 // expanded
        };

        // check to make sure our new line format isn't bigger than the console
        let line_width = line_format.line_width();
        if line_width > console_width && (line_format.title_width-(line_width-console_width)) > 0 {
            line_format.title_width -= line_width - console_width;
        }

        // debuging
        // println!("console width: {}, line width: {}", console_width, line_format.line_width());
        // println!("id: {}, title: {}, status: {}, last: {}", line_format.id_width, line_format.title_width, line_format.status_width, line_format.touched_width);

        line_format
    }

    fn line_width(&self) -> uint {
        self.id_width+self.title_width+self.status_width+self.touched_width+(3*self.colsep)
    }
}

#[derive(RustcDecodable, Clone)]
pub struct ThecaItem {
    id: uint,
    title: String,
    status: String,
    body: String,
    last_touched: String
}

impl Encodable for ThecaItem {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), E::Error> {
        match *self {
            ThecaItem{id: ref p_id, title: ref p_title, status: ref p_status, body: ref p_body, last_touched: ref p_last_touched} => {
                encoder.emit_struct("ThecaItem", 1, |encoder| {
                    try!(encoder.emit_struct_field("id", 0u, |encoder| p_id.encode(encoder)));
                    try!(encoder.emit_struct_field("title", 1u, |encoder| p_title.encode(encoder)));
                    try!(encoder.emit_struct_field("status", 2u, |encoder| p_status.encode(encoder)));
                    try!(encoder.emit_struct_field("body", 3u, |encoder| p_body.encode(encoder)));
                    try!(encoder.emit_struct_field("last_touched", 4u, |encoder| p_last_touched.encode(encoder)));
                    Ok(())
                })
            }
        }
    }
}

impl ThecaItem {
    fn encrypt(&mut self, password: &str) {
        let (key, iv) = password_to_key(password);
        self.title = cipher_to_str(&encrypt(self.title.as_bytes(), key.as_slice(), iv.as_slice()).ok().unwrap());
        self.body = cipher_to_str(&encrypt(self.body.as_bytes(), key.as_slice(), iv.as_slice()).ok().unwrap());
        self.status = cipher_to_str(&encrypt(self.status.as_bytes(), key.as_slice(), iv.as_slice()).ok().unwrap());
        self.last_touched = cipher_to_str(&encrypt(self.last_touched.as_bytes(), key.as_slice(), iv.as_slice()).ok().unwrap());
    }

    fn decrypt(&mut self, password: &str) {
        let (key, iv) = password_to_key(password);
        self.title = String::from_utf8(decrypt(cipher_to_buf(&self.title).as_slice(), key.as_slice(), iv.as_slice()).ok().unwrap()).unwrap();
        self.body = String::from_utf8(decrypt(cipher_to_buf(&self.body).as_slice(), key.as_slice(), iv.as_slice()).ok().unwrap()).unwrap();
        self.status = String::from_utf8(decrypt(cipher_to_buf(&self.status).as_slice(), key.as_slice(), iv.as_slice()).ok().unwrap()).unwrap();
        self.last_touched = String::from_utf8(decrypt(cipher_to_buf(&self.last_touched).as_slice(), key.as_slice(), iv.as_slice()).ok().unwrap()).unwrap();
    }

    fn print(&self, line_format: &LineFormat, args: &Args) {
        let column_seperator: String = repeat(' ').take(line_format.colsep).collect();
        print!("{}", format_field(&self.id.to_string(), line_format.id_width, false));
        print!("{}", column_seperator);
        if !self.body.is_empty() {
            print!("(+) {}", format_field(&self.title, line_format.title_width-4, true));
        } else {
            print!("{}", format_field(&self.title, line_format.title_width, true));
        }
        print!("{}", column_seperator);
        if args.flag_c && self.status.len() > 0 {
            print!("{}", format_field(&self.status.chars().nth(0).unwrap().to_string(), line_format.status_width, false));
        } else {
            print!("{}", format_field(&self.status, line_format.status_width, false));
        }
        print!("{}", column_seperator);
        print!("{}", format_field(&self.last_touched, line_format.touched_width, false));
        print!("\n");
    }
}

// ALL the encryption functions ^_^
fn encrypt(data: &[u8], key: &[u8], iv: &[u8]) -> Result<Vec<u8>, symmetriccipher::SymmetricCipherError> {
    let mut encryptor = aes::cbc_encryptor(
            aes::KeySize::KeySize256,
            key,
            iv,
            blockmodes::PkcsPadding);

    let mut final_result = Vec::<u8>::new();
    let mut read_buffer = buffer::RefReadBuffer::new(data);
    let mut buffer = [0; 4096];
    let mut write_buffer = buffer::RefWriteBuffer::new(&mut buffer);

    loop {
        let result = try!(encryptor.encrypt(&mut read_buffer, &mut write_buffer, true));

        final_result.push_all(write_buffer.take_read_buffer().take_remaining());

        match result {
            BufferResult::BufferUnderflow => break,
            BufferResult::BufferOverflow => { }
        }
    }

    Ok(final_result)
}

fn decrypt(encrypted_data: &[u8], key: &[u8], iv: &[u8]) -> Result<Vec<u8>, symmetriccipher::SymmetricCipherError> {
    let mut decryptor = aes::cbc_decryptor(
            aes::KeySize::KeySize256,
            key,
            iv,
            blockmodes::PkcsPadding);

    let mut final_result = Vec::<u8>::new();
    let mut read_buffer = buffer::RefReadBuffer::new(encrypted_data);
    let mut buffer = [0; 4096];
    let mut write_buffer = buffer::RefWriteBuffer::new(&mut buffer);

    loop {
        let result = try!(decryptor.decrypt(&mut read_buffer, &mut write_buffer, true));
        final_result.push_all(write_buffer.take_read_buffer().take_remaining());
        match result {
            BufferResult::BufferUnderflow => break,
            BufferResult::BufferOverflow => { }
        }
    }

    Ok(final_result)
}

fn password_to_key(p: &str) -> (Vec<u8>, Vec<u8>) {
    let mut salt_sha = Sha256::new();
    salt_sha.input(p.as_bytes());
    let salt = salt_sha.result_str();

    let mut mac = Hmac::new(Sha256::new(), p.as_bytes());
    let mut key: Vec<u8> = repeat(0).take(32).collect();
    let mut iv: Vec<u8> = repeat(0).take(16).collect();

    pbkdf2(&mut mac, salt.as_bytes(), 2056, key.as_mut_slice());
    pbkdf2(&mut mac, salt.as_bytes(), 1028, iv.as_mut_slice());

    (key, iv)
}

// for saving cipher
fn cipher_to_str(buf: &Vec<u8>) -> String {
    buf.to_base64(MIME)
}

// for decrypting cipher
fn cipher_to_buf(cipher: &String) -> Vec<u8> {
    cipher.as_bytes().from_base64().unwrap()
}

#[derive(RustcDecodable)]
pub struct ThecaProfile {
    encrypted: bool,
    notes: Vec<ThecaItem>
}

impl Encodable for ThecaProfile {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), E::Error> {
        match *self {
            ThecaProfile{encrypted: ref p_encrypted, notes: ref p_notes} => {
                encoder.emit_struct("ThecaProfile", 1, |encoder| {
                    try!(encoder.emit_struct_field("encrypted", 0u, |encoder| p_encrypted.encode(encoder)));
                    try!(encoder.emit_struct_field("notes", 1u, |encoder| p_notes.encode(encoder)));
                    Ok(())
                })
            }
        }
    }
}

impl ThecaProfile {
    // this should be a method of ThecaProfile
    fn new(args: &Args) -> Result<ThecaProfile, String> {
        if args.cmd_new_profile {
            Ok(ThecaProfile {
                encrypted: args.flag_encrypted,
                notes: vec![]
            })
        } else {
            // set profile folder
            let mut profile_path = find_profile_folder(args);

            // set profile name
            if !args.flag_p.is_empty() {
                profile_path.push(args.flag_p[0].to_string() + ".json");
            } else {
                profile_path.push("default".to_string() + ".json");
            }

            // attempt to read profile
            match profile_path.is_file() {
                false => {
                    if profile_path.exists() {
                        Err(format!("{} is not a file.", profile_path.display()))
                    } else {
                        Err(format!("{} does not exist.", profile_path.display()))
                    }
                }
                true => {
                    let mut file = match File::open_mode(&profile_path, Open, Read) {
                        Ok(t) => t,
                        Err(e) => panic!("{}", e.desc)
                    };
                    let contents = match file.read_to_string() {
                        Ok(t) => t,
                        Err(e) => panic!("{}", e.desc)
                    };
                    let decoded: ThecaProfile = match json::decode(contents.as_slice()) {
                        Ok(s) => s,
                        Err(e) => panic!("Invalid JSON in {}. {}", profile_path.display(), e)
                    };
                    Ok(decoded)
                }
            }
        }
    }

    fn save_to_file(&mut self, args: &Args) {
        // set profile folder
        let mut profile_path = find_profile_folder(args);

        // set file name
        if !args.flag_p.is_empty() {
            profile_path.push(args.flag_p[0].to_string() + ".json");
        } else if args.cmd_new_profile {
            profile_path.push(args.arg_name.to_string() + ".json");
        } else {
            profile_path.push("default".to_string() + ".json");
        }

        // open file
        let mut file = match File::open_mode(&profile_path, Truncate, Write) {
            Ok(f) => f,
            Err(e) => panic!("File error: {}", e)
        };

        // encode to buffer
        let mut buffer: Vec<u8> = Vec::new();
        {
            let mut encoder = json::PrettyEncoder::new(&mut buffer);
            self.encode(&mut encoder).ok().expect("JSON encoding error.");
        }

        // write buffer to file
        file.write(buffer.as_slice()).ok().expect(format!("Couldn't write to {}", profile_path.display()).as_slice());
    }

    fn add_item(&mut self, a_title: String, a_status: String, a_body: String) {
        let new_id = self.notes.last().unwrap().id;
        self.notes.push(ThecaItem {
            id: new_id + 1,
            title: a_title,
            status: a_status,
            body: a_body,
            last_touched: strftime("%F %T", &now_utc()).ok().unwrap()
        });
        if self.encrypted {
            let item_pos = self.notes.iter().position(|n| n.id == new_id).unwrap();
            self.notes[item_pos].encrypt("weewoo");
        }
        println!("added");
    }

    fn delete_item(&mut self, id: uint) {
        let remove = self.notes.iter()
            .position(|n| n.id == id)
            .map(|e| self.notes.remove(e))
            .is_some();
        match remove {
            true => {
                println!("removed");
            }
            false => {
                println!("not found");
            }
        }
    }

    fn edit_item(&mut self, id: uint, args: &Args) {
        let item_pos: uint = self.notes.iter()
            .position(|n| n.id == id)
            .unwrap();
        if !args.arg_title.is_empty() {
            // change title
            self.notes[item_pos].title = args.arg_title.replace("\n", "").to_string();
        } else if args.flag_started || args.flag_urgent || args.flag_none {
            // change status
            if args.flag_started {
                self.notes[item_pos].status = STARTED.to_string();
            } else if args.flag_urgent {
                self.notes[item_pos].status = URGENT.to_string();
            } else if args.flag_none {
                self.notes[item_pos].status = NOSTATUS.to_string();
            }
        } else if !args.flag_b.is_empty() || args.flag_editor || args.cmd__ {
            // change body
            if !args.flag_b.is_empty() {
                self.notes[item_pos].body = args.flag_b[0].to_string();
            } else if args.flag_editor {
                self.notes[item_pos].body = drop_to_editor(&self.notes[item_pos].body);
            } else if args.cmd__ {
                stdin().lock().read_to_string().unwrap();
            }
        }
        // update last_touched
        self.notes[item_pos].last_touched = strftime("%F %T", &now_utc()).ok().unwrap();
        println!("edited")
    }

    fn print_header(&mut self, line_format: &LineFormat) {
        let column_seperator: String = repeat(' ').take(line_format.colsep).collect();
        let header_seperator: String = repeat('-').take(line_format.line_width()).collect();
        println!(
            "{1}{0}{2}{0}{3}{0}{4}\n{5}",
            column_seperator,
            format_field(&"id".to_string(), line_format.id_width, false),
            format_field(&"title".to_string(), line_format.title_width, false),
            format_field(&"status".to_string(), line_format.status_width, false),
            format_field(&"last touched".to_string(), line_format.touched_width, false),
            header_seperator
        );
    }

    fn view_item(&mut self, id: uint, args: &Args, body: bool) {
        let note_pos = self.notes.iter().position(|n| n.id == id).unwrap();
        let line_format = LineFormat::new(&vec![self.notes[note_pos].clone()], args);
        if !args.flag_c {
            self.print_header(&line_format);
        }
        self.notes[note_pos].print(&line_format, args);
        if body && !self.notes[note_pos].body.is_empty() {
            println!("{}", self.notes[note_pos].body);
        }
    }

    fn list_items(&mut self, args: &Args) {
        let line_format = LineFormat::new(&self.notes, args);
        if !args.flag_c {
            self.print_header(&line_format);
        }
        let list_range = if !args.flag_l.is_empty() {
            args.flag_l[0]
        } else {
            self.notes.len()
        };
        for i in range(0, list_range) {
            self.notes[i].print(&line_format, args);
        }
    }

    fn search_items(&mut self, regex_pattern: &str, body: bool, args: &Args) {
        let re = match Regex::new(regex_pattern) {
            Ok(r) => r,
            Err(e) => panic!("{}", e)
        };
        let notes: Vec<ThecaItem> = match body {
            true => self.notes.iter().filter(|n| re.is_match(n.body.as_slice())).map(|n| n.clone()).collect(),
            false => self.notes.iter().filter(|n| re.is_match(n.title.as_slice())).map(|n| n.clone()).collect()
        };
        let line_format = LineFormat::new(&notes, args);
        if !args.flag_c {
            self.print_header(&line_format);
        }
        for i in range(0, notes.len()) {
            notes[i].print(&line_format, args);
            if body {
                println!("{}", notes[i].body);
            }
        }
    }
}

fn format_field(value: &String, width: uint, truncate: bool) -> String {
    if value.len() > width && width > 3 && truncate {
        format!("{: <1$.1$}...", value, width-3)
    } else {
        format!("{: <1$.1$}", value, width)
    }
}

fn find_profile_folder(args: &Args) -> Path {
    if !args.flag_profiles_folder.is_empty() {
        Path::new(args.flag_profiles_folder[0].to_string())
    } else {
        match homedir() {
            Some(ref p) => p.join(".theca"),
            None => Path::new(".").join(".theca")
        }
    }
}

fn drop_to_editor(contents: &String) -> String {
    // this could probably be prettyified tbh!

    // setup temporary directory
    let tmpdir = match TempDir::new("theca") {
        Ok(dir) => dir,
        Err(e) => panic!("couldn't create temporary directory: {}", e)
    };
    // setup temporary file to write/read
    let tmppath = tmpdir.path().join("something-unique");
    let mut tmpfile = match File::open_mode(&tmppath, Open, ReadWrite) {
        Ok(f) => f,
        Err(e) => panic!("File error: {}", e)
    };
    tmpfile.write_line(contents.as_slice()).ok().expect("Failed to write line to temp file");
    // we now have a temp file, at `tmppath`, that contains `contents`
    // first we need to know which onqe
    let editor = match getenv("VISUAL") {
        Some(val) => val,
        None => {
            match getenv("EDITOR") {
                Some(val) => val,
                None => panic!("Neither $VISUAL nor $EDITOR is set.")
            }
        }
    };
    // lets start `editor` and edit the file at `tmppath`
    // first we need to set STDIN, STDOUT, and STDERR to those that theca is
    // currently using so we can display the editor
    let mut editor_command = Command::new(editor);
    editor_command.arg(tmppath.display().to_string());
    editor_command.stdin(InheritFd(STDIN_FILENO));
    editor_command.stdout(InheritFd(STDOUT_FILENO));
    editor_command.stderr(InheritFd(STDERR_FILENO));
    let editor_proc = editor_command.spawn();
    match editor_proc.ok().expect("Couldn't launch editor").wait().is_ok() {
        true => {
            // finished editing, time to read `tmpfile` for the final output
            // seek to start of `tmpfile`
            tmpfile.seek(0, SeekSet).ok().expect("Can't seek to start of temp file");
            tmpfile.read_to_string().unwrap()
        }
        false => panic!("The editor broke")
    }
}

fn main() {
    let mut args: Args = Docopt::new(USAGE)
                            .and_then(|d| d.decode())
                            .unwrap_or_else(|e| e.exit());

    // is anything stored in the ENV?
    args.check_env();

    // Setup a ThecaProfile struct
    let mut profile = match ThecaProfile::new(&args) {
        Ok(p) => p,
        Err(e) => panic!("{}", e)
    };

    // see what root command was used
    if args.cmd_add {
        // add a item
        let title = args.arg_title.replace("\n", "").to_string();
        let status = if args.flag_started {
            STARTED.to_string()
        } else if args.flag_urgent {
            URGENT.to_string()
        } else {
            NOSTATUS.to_string()
        };
        let body = if !args.flag_b.is_empty() {
            args.flag_b[0].to_string()
        } else if args.flag_editor {
            drop_to_editor(&"".to_string())
        } else if args.cmd__ {
            stdin().lock().read_to_string().unwrap()
        } else {
            "".to_string()
        };
        profile.add_item(title, status, body);
    } else if args.cmd_edit {
        // edit a item
        let id = args.arg_id[0];
        profile.edit_item(id, &args);
    } else if args.cmd_del {
        // delete a item
        let id = args.arg_id[0];
        profile.delete_item(id);
    } else if args.flag_v {
        // display theca version
        println!("theca v{}", VERSION);
    } else if args.cmd_view {
        // view full item
        profile.view_item(args.arg_id[0], &args, true);
    } else if args.cmd_search || args.cmd_search_body {
        // search for an item
        match args.cmd_search {
            true => profile.search_items(args.arg_pattern.as_slice(), false, &args),
            false => profile.search_items(args.arg_pattern.as_slice(), true, &args)
        }
    } else if !args.cmd_view && !args.arg_id.is_empty() {
        // view short item
        profile.view_item(args.arg_id[0], &args, false);
    } else if !args.cmd_new_profile {
        // this should be the default for nothing
        profile.list_items(&args);
    }

    // save altered profile back to disk
    // this should only be triggered by commands that make transactions to the profile
    if args.cmd_add || args.cmd_edit || args.cmd_del || args.cmd_new_profile {
        profile.save_to_file(&args);
    }
}