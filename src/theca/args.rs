use clap::{Parser, Subcommand};

#[derive(Parser, Debug, Clone)]
#[command(name = "theca")]
#[command(version = "2.0.0")]
#[command(about = "a simple, fully featured, command line note taking tool", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// View note by ID (positional)
    #[arg(required = false)]
    pub id: Option<usize>,

    /// Profile to use
    #[arg(short, long, default_value = "default")]
    pub profile: String,

    /// Profile folder to use
    #[arg(long, env = "THECA_PROFILE_FOLDER")]
    pub profile_folder: Option<String>,

    /// Encryption key
    #[arg(short, long, env = "THECA_KEY")]
    pub key: Option<String>,

    /// Encrypted profile (flag)
    #[arg(long)]
    pub encrypted: bool,
    
    /// Do not ask for confirmation
    #[arg(short, long)]
    pub yes: bool,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Add a new note
    Add {
        /// Title of the note
        title: String,

        /// Body of the note
        #[arg(default_value = "")]
        body: String,

        /// Status of the note (Started, Urgent, Blank)
        #[arg(short, long)]
        status: Option<String>,

        /// Use editor to write body
        #[arg(short, long)]
        editor: bool,
    },
    
    /// Edit an existing note
    Edit {
        /// ID of the note to edit
        id: usize,

        /// New title
        #[arg(short, long)]
        title: Option<String>,
        
        /// New body
        #[arg(short, long)]
        body: Option<String>,

        /// New status
        #[arg(short, long)]
        status: Option<String>,

        /// Use editor
        #[arg(short, long)]
        editor: bool,
    },

    /// Delete a note
    Del {
        /// ID(s) of the note to delete
        #[arg(required = true)]
        id: Vec<usize>,
    },
    
    /// List profiles
    ListProfiles,

    /// Create a new profile
    NewProfile {
        /// Name of the new profile
        name: String,
    },

    /// Transfer a note to another profile
    Transfer {
        /// Note ID
        id: usize,
        /// Target profile name
        target_profile: String,
    },

    /// Encrypt the current profile
    EncryptProfile {
        /// New key (optional, will prompt if missing)
        #[arg(long)]
        new_key: Option<String>,
    },

    /// Decrypt the current profile
    DecryptProfile,

    /// Search notes
    Search {
        /// Pattern to search for
        pattern: String,
        
        /// Search in body as well
        #[arg(short = 'b', long)]
        search_body: bool,

        /// Use regex
        #[arg(short, long)]
        regex: bool,

        /// Limit results
        #[arg(short, long)]
        limit: Option<usize>,
    },

    /// Show profile info
    Info,

    /// Clear all notes
    Clear,

    /// List notes (default if no command)
    List {
        /// Limit results
        #[arg(short, long)]
        limit: Option<usize>,

        /// Sort by date
        #[arg(short, long)]
        datesort: bool,

        /// Reverse sort
        #[arg(short, long)]
        reverse: bool,

        /// Output as YAML
        #[arg(long)]
        yaml: bool,

        /// Condensed output
        #[arg(short, long)]
        condensed: bool,

        /// Filter by status
        #[arg(long)]
        status: Option<String>,
    }
}
