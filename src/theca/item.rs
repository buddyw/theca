use std::fmt;
use std::iter::repeat;
use std::io::{self, Write};

use crate::lineformat::LineFormat;
use crate::utils::{format_field, localize_last_touched_string};
use crate::errors::Result;
use serde::{Serialize, Deserialize};

/// Represents a note within a profile
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Item {
    pub id: usize,
    pub title: String,
    pub status: Status,
    pub body: String,
    pub last_touched: String,
}

impl Item {
    /// print a note as a line
    pub fn print(&self, line_format: &LineFormat, search_body: bool) -> Result<()> {
        self.write(&mut io::stdout(), line_format, search_body)
    }

    pub fn write<T: Write>(&self,
                           output: &mut T,
                           line_format: &LineFormat,
                           search_body: bool)
                           -> Result<()> {
        let column_seperator: String = repeat(' ')
                                           .take(line_format.colsep)
                                           .collect();
        write!(output,
                    "{}",
                    format_field(&self.id.to_string(), line_format.id_width, false))?;
        write!(output, "{}", column_seperator)?;
        if !self.body.is_empty() && !search_body {
            write!(output,
                        "{}",
                        format_field(&self.title, if line_format.title_width > 4 { line_format.title_width - 4 } else { 0 }, true))?;
            write!(output, "{}", format_field(&" (+)".to_string(), 4, false))?;
        } else {
            write!(output,
                        "{}",
                        format_field(&self.title, line_format.title_width, true))?;
        }
        write!(output, "{}", column_seperator)?;
        if line_format.status_width != 0 {
            write!(output,
                        "{}",
                        format_field(&format!("{}", self.status),
                                     line_format.status_width,
                                     false))?;
            write!(output, "{}", column_seperator)?;
        }
        writeln!(output,
                      "{}",
                      format_field(&localize_last_touched_string(&*self.last_touched)?,
                                   line_format.touched_width,
                                   false))?;
        if search_body {
            for l in self.body.lines() {
                writeln!(output, "\t{}", l)?;
            }
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub enum Status {
    Blank,
    Started,
    Urgent,
    Done,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,
               "{}",
               match *self {
                   Status::Blank => "",
                   Status::Started => "Started",
                   Status::Urgent => "Urgent",
                   Status::Done => "Done",
               })
    }
}
