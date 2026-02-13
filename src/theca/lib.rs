pub mod args;
pub mod crypt;
pub mod errors;
pub mod item;
pub mod lineformat;
pub mod profile;
pub mod utils;

use clap::Parser;
use args::{Cli, Commands};
use profile::{Profile, ProfileFlags};
use errors::Result;

pub fn r#run() -> Result<()> {
    let cli = Cli::parse();

    // Determine the profile to load.
    // If command is NewProfile, we still load "default" or whatever --profile says?
    // Actually, Profile::new handles loading or creating.
    // If NewProfile command is used, we might be creating a NEW one, but Profile::new is used to load the *current* context.
    // However, for NewProfile, we don't necessarily need a current context unless we are copying settings?
    // But theca architecture seems to be centered around "Load a profile, then do something".
    // Except NewProfile creates a new one.
    
    // Logic:
    // If NewProfile, we don't strictly need to load `cli.profile`.
    // But let's follow standard flow: Load `cli.profile` (which defaults to "default"), then execute command.
    // UNLESS `cli.profile` doesn't exist AND we are running `new-profile`.
    // But `new-profile` creates *another* profile.
    
    // We will load the profile specified by `--profile` (or default).
    // If it doesn't exist, `Profile::new` will prompt to create it (unless `new_profile` arg to Profile::new is true).
    
    // Wait, `cli.command` might be `NewProfile`.
    let is_new_profile_cmd = matches!(cli.command, Some(Commands::NewProfile { .. }));
    
    // We only want `Profile::new` to enter "create new" mode if we are actually creating the profile specified in `cli.profile`.
    // `theca new-profile foo` -> Creates "foo". `cli.profile` is "default".
    // We should NOT create "default" if it doesn't exist, just to create "foo".
    // But `Profile::new` is designed to load `cli.profile`.
    
    // Let's implement lazy loading or just load.
    // If we are running `NewProfile`, we can skip loading `cli.profile`?
    // But `run` structure expects a profile.
    
    // Let's just load it. If "default" doesn't exist, `Profile::new` asks to create it.
    // User might say no. Then we fail?
    // FIXME: This is a slight UX regression if user just wants to run `theca new-profile` on fresh install.
    // They will be asked to create "default" first.
    // We can pass `false` for `new_profile` to `Profile::new`?
    // `Profile::new` signature: `new_profile: bool`.
    // If true, it calls `from_scratch`.
    // If false, `from_existing`.
    
    // If we are running `NewProfile` command, we probably shouldn't trigger creation of `cli.profile`.
    // But `lib.rs` logic I wrote before passed `matches!(cli.command, NewProfile)` as `new_profile` arg to `Profile::new`.
    // This means if I run `theca new-profile foo`, `Profile::new("default", ..., true)` is called.
    // `from_scratch("default")` is called.
    // This creates "default" (or asks).
    // Then later we create "foo".
    // Use `false` for `new_profile` when running `NewProfile` command, to avoid forcing creation of the *current* profile?
    
    let (mut profile, fingerprint) = if !is_new_profile_cmd {
         Profile::new(
            &cli.profile,
            &cli.profile_folder,
            cli.key.as_ref(), 
            false, // Don't force create unless... wait.
            // If `cli.profile` doesn't exist, `from_existing` fails.
            // `from_scratch` creates.
            // When does user want to create `cli.profile`?
            // When they run `theca --profile foo` and it doesn't exist?
            // Original logic used `args.cmd_new_profile`. 
            // `args.cmd_new_profile` was true if `new-profile` command was used.
            // So original logic: `theca new-profile foo` -> `Profile::new(..., true)`.
            // `Profile::new` used `args.flag_profile`?
            // `args.flag_profile` is global.
            // `new-profile` sets `args.cmd_new_profile`.
            // So `theca new-profile foo` -> `Profile::new("default", ..., true)`.
            // `from_scratch` creates "default".
            // Then `parse_cmds` handled `new_profile`.
            // `check cmd_new_profile`.
            // `args.arg_name` used.
            // `println!("creating profile '{}'", args.arg_name[0])`.
            // `save_to_file` used `cmd_new_profile` to save to `arg_name`.
            
            // So original logic created/loaded "default", but then Saved As "foo".
            // This effectively "cloned" default to foo? Or created empty?
            // `Profile::new` returns empty profile if `from_scratch`.
            
            // So `theca new-profile foo`:
            // 1. `Profile::new(..., true)` -> `from_scratch` -> returns empty profile named "default" (in struct, but assumes "default" path).
            // 2. `parse_cmds` -> `save_to_file` -> switches name to "foo".
            // So it creates a fresh profile "foo".
            
            // It did NOT create "default" on disk if `from_scratch` just returned struct?
            // `from_scratch` calls `create_dir` (folder) but doesn't write profile file.
            // It returns `Profile` struct.
            // `save_to_file` writes it.
            
            // So my previous `lib.rs` logic was close.
            cli.encrypted,
            cli.yes,
        )?
    } else {
        // Special case for NewProfile: We want an empty profile to start with.
        // We don't want to load from disk.
        Profile::new(
            &cli.profile, // Name doesn't matter much here if we ignore it later
            &cli.profile_folder,
            cli.key.as_ref(),
            true, // from_scratch
            cli.encrypted,
            cli.yes
        )?
    };

    match &cli.command {
        Some(Commands::Add { title, body, status, editor }) => {
            profile.add_note(title, 
                             &[body.clone()], 
                             utils::extract_status(status.clone())?, 
                             false, 
                             *editor,
                             true)?;
             profile.save_to_file(&cli.profile, &cli.profile_folder, cli.key.as_ref(), false, cli.yes, &fingerprint)?;

        }
        Some(Commands::Edit { id, title, body, status, editor }) => {
             let flags = ProfileFlags {
                editor: *editor,
                encrypted: cli.encrypted,
                yes: cli.yes,
                ..Default::default()
            };
            // Map Some(String) -> Option<Status>
            let st = if let Some(s) = status {
                utils::extract_status(Some(s.clone()))?
            } else {
                None
            };
            
            profile.edit_note(*id, title, body, &st, false, flags)?;
            profile.save_to_file(&cli.profile, &cli.profile_folder, cli.key.as_ref(), false, cli.yes, &fingerprint)?;

        }
        Some(Commands::Del { id }) => {
            profile.delete_note(id);
            profile.save_to_file(&cli.profile, &cli.profile_folder, cli.key.as_ref(), false, cli.yes, &fingerprint)?;
        }
        Some(Commands::Transfer { id, target_profile }) => {
             // transfer_note saves both?
             profile.transfer_note(*id, target_profile, &cli.profile, &cli.profile_folder, cli.key.as_ref(), cli.encrypted, cli.yes)?;
             // transfer_note in profile.rs removes from self and saves target.
             // We need to save self.
             profile.save_to_file(&cli.profile, &cli.profile_folder, cli.key.as_ref(), false, cli.yes, &fingerprint)?;
        }
        Some(Commands::NewProfile { name }) => {
             // profile is empty from `from_scratch`
             // Save it as `name`.
             let key = if cli.encrypted {
                 if let Some(k) = &cli.key {
                     Some(k.clone())
                 } else {
                     Some(utils::get_new_password()?)
                 }
             } else {
                 None
             };
             profile.save_to_file(name, &cli.profile_folder, key.as_ref(), true, cli.yes, &0)?;
             println!("created profile '{}'", name);
        }
        Some(Commands::EncryptProfile { new_key }) => {
             if !profile.encrypted {
                  let key = match new_key {
                      Some(k) => k.clone(),
                      None => utils::get_new_password()?,
                  };
                  let mut new_profile = profile.clone();
                  new_profile.encrypted = true;
                  new_profile.save_to_file(&cli.profile, &cli.profile_folder, Some(&key), false, cli.yes, &0)?;
                  println!("encrypting '{}'", cli.profile);
             } else {
                 println!("Profile '{}' is already encrypted.", cli.profile);
             }
        }
        Some(Commands::DecryptProfile) => {
             if profile.encrypted {
                  let mut new_profile = profile.clone();
                  new_profile.encrypted = false;
                  new_profile.save_to_file(&cli.profile, &cli.profile_folder, None, false, cli.yes, &0)?;
                  println!("decrypting '{}'", cli.profile);
             } else {
                 println!("Profile '{}' is not encrypted.", cli.profile);
             }
        }
        Some(Commands::Search { pattern, search_body, regex, limit }) => {
             let flags = ProfileFlags {
                search_body: *search_body,
                regex: *regex,
                condensed: false, 
                yaml: false, 
                ..Default::default()
            };
            profile.search_notes(pattern, limit.unwrap_or(0), flags, None)?;
        }
        Some(Commands::ListProfiles) => {
            let folder = utils::find_profile_folder(&cli.profile_folder)?;
            utils::profiles_in_folder(&folder)?;
        }
        Some(Commands::Info) => {
            profile.stats(&cli.profile)?;
        }
        Some(Commands::Clear) => {
            profile.clear(cli.yes)?;
            profile.save_to_file(&cli.profile, &cli.profile_folder, cli.key.as_ref(), false, cli.yes, &fingerprint)?;
        }
        Some(Commands::List { limit, datesort, reverse, yaml, condensed, status }) => {
             let flags = ProfileFlags {
                yaml: *yaml,
                condensed: *condensed,
                datesort: *datesort,
                reverse: *reverse,
                ..Default::default()
             };
             let st = if let Some(s) = status {
                utils::extract_status(Some(s.clone()))?
             } else {
                None
             };
             profile.list_notes(limit.unwrap_or(0), flags, st)?;
        }
        None => {
            if let Some(id) = cli.id {
                profile.view_note(id, false, false)?;
            } else {
                // Default list
                let flags = ProfileFlags::default(); // defaults to false for json/condensed etc
                profile.list_notes(0, flags, None)?;
            }
        }
    }

    Ok(())
}
