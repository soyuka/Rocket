#![feature(plugin, decl_macro)]
#![plugin(rocket_codegen)]

extern crate rocket_contrib;
extern crate rocket;
extern crate serde_json;
extern crate pretty_bytes;
extern crate chrono;
extern crate mime_guess;

#[macro_use] extern crate serde_derive;

#[cfg(test)] mod tests;

use std::collections::HashMap;

use mime_guess::guess_mime_type_opt;
use rocket::request::{self, Request, FromRequest, FromSegments};
use rocket::http::uri::{Segments, SegmentError, Uri};
use rocket::outcome::Outcome;
use std::path::PathBuf;
use std::path::Path;
// use rocket::response::Redirect;
use rocket_contrib::Template;
use rocket::response::{NamedFile, Failure};
use rocket::http::Status;
use pretty_bytes::converter::convert;

// use std::io;
use std::fs;
use std::time::SystemTime;
use chrono::DateTime;
use chrono::offset::Utc;

#[derive(Serialize)]
struct TemplateContext {
    path: String,
    items: Vec<TemplateFile>
}

#[derive(Serialize)]
struct TemplateFile {
    name: String,
    size: String,
    file_type: String,
    mtime: String,
    is_dir: bool,
    is_symlink: bool,
    path: String
}

static DEFAULT_SIZE: &str = "-";
static DIRECTORY_FILE_TYPE: &str = "directory";
static DEFAULT_MIME_TYPE: &str = "application/unknown";
static HOME: &str = "/home/abluchet";

struct DirectoryPath {
    inner: PathBuf
}

impl DirectoryPath {
    pub fn new(path_buf: PathBuf) -> DirectoryPath {
        DirectoryPath { inner: path_buf }
    }

    pub fn as_path(&self) -> &Path {
        self.inner.as_path()
    }

    pub fn is_dir(&self) -> bool {
        self.inner.is_dir()
    }

    pub fn to_str(&self) -> Option<&str> {
        self.inner.to_str()
    }

    pub fn from_segments(segments: Segments) -> Result<DirectoryPath, SegmentError> {
        let mut buf = PathBuf::new();
        for segment in segments {
            let decoded = Uri::percent_decode(segment.as_bytes())
                .map_err(|e| SegmentError::Utf8(e))?;

            if decoded == ".." {
                buf.pop();
            } else if decoded.contains("..") {
                return Err(SegmentError::BadChar('.'))
            } else if decoded.starts_with('*') {
                return Err(SegmentError::BadStart('*'))
            } else if decoded.ends_with(':') {
                return Err(SegmentError::BadEnd(':'))
            } else if decoded.ends_with('>') {
                return Err(SegmentError::BadEnd('>'))
            } else if decoded.ends_with('<') {
                return Err(SegmentError::BadEnd('<'))
            } else if decoded.contains('/') {
                return Err(SegmentError::BadChar('/'))
            } else if cfg!(windows) && decoded.contains('\\') {
                return Err(SegmentError::BadChar('\\'))
            } else {
                buf.push(&*decoded)
            }
        }

        Ok(DirectoryPath::new(buf))
    }
}

impl AsRef<Path> for DirectoryPath {
    fn as_ref(&self) -> &Path {
        self.inner.as_path()
    }
}

impl<'a, 'r> FromRequest<'a, 'r> for DirectoryPath {
    type Error = ();
    fn from_request(request: &'a Request<'r>) -> request::Outcome<Self, ()> {
        let home = PathBuf::from(HOME);
        match request.get_segments::<Segments>(0) {
            Ok(segments) => {
                let path = home.join(DirectoryPath::from_segments(segments).unwrap());

                if !path.exists() {
                    return Outcome::Failure((Status::NotFound, ()));
                }

                if path.is_dir() {
                    return Outcome::Success(DirectoryPath::new(path));
                }

                return Outcome::Forward(())
            },
            Err(_reason) => Outcome::Success(DirectoryPath::new(home))

        }
    }
}

impl<'a> FromSegments<'a> for DirectoryPath {
    type Error = Failure;

    fn from_segments(segments: Segments<'a>) -> Result<DirectoryPath, Failure> {
        let path = Path::new(HOME).join(DirectoryPath::from_segments(segments).unwrap());

        if !path.exists() {
            return Err(Failure(Status::NotFound));
        }

        return Ok(DirectoryPath::new(path));
    }
}

#[get("/css/<file..>")]
fn css(file: PathBuf) -> Option<NamedFile> {
    NamedFile::open(Path::new("css/").join(file)).ok()
}

/**
 * Directory listing
 */
#[get("/")]
fn index(path: DirectoryPath) -> Result<Template, Failure> {
    list(path)
}

#[get("/<path..>", rank = 2)]
fn dir(path: DirectoryPath) -> Result<Result<Template, Failure>, Option<NamedFile>> {
    if path.is_dir() {
        Ok(list(path))
    } else {
        Err(download(path))
    }
}

#[get("/<path..>", rank = 3)]
fn download(path: DirectoryPath) -> Option<NamedFile> {
    NamedFile::open(path).ok()
}

#[catch(404)]
fn not_found(req: &Request) -> Template {
    let mut map = HashMap::new();
    map.insert("path", req.uri().as_str());
    Template::render("error/404", &map)
}

fn rocket() -> rocket::Rocket {
    rocket::ignite()
        .mount("/", routes![index, dir, css])
        .attach(Template::fairing())
        .catch(catchers![not_found])
}

fn main() {
    rocket().launch();
}

fn list(path: DirectoryPath) -> Result<Template, Failure> {
    match readdir(path.as_path()) {
        Ok(items) => {
            let path = String::from(path.to_str().unwrap_or("/"));
            let context = TemplateContext { path, items };
            Ok(Template::render("index", &context))
        },
        // Redirect to 400 on readdir error
        Err(_reason) => Err(Failure(Status::BadRequest))
    }
}

/// Todo move to lib
fn get_file_size(metadata: &fs::Metadata) -> String {
    let size = metadata.len() as f64;
    return convert(size);
}

fn get_time(metadata: &fs::Metadata) -> std::io::Result<String> {
    let date: DateTime<Utc> = metadata.modified().unwrap_or(SystemTime::now()).into();

    Ok(date.format("%d/%m/%Y %Hh%M").to_string())
}

fn dir_entry_to_template_file(entry: fs::DirEntry) -> Option<TemplateFile> {
    let metadata = entry.metadata().unwrap();
    let path = entry.path();
    let file_type = metadata.file_type();
    let is_dir = file_type.is_dir();

    Some(TemplateFile {
        size:
            if is_dir {
                String::from(DEFAULT_SIZE)
            } else {
                get_file_size(&metadata)
            },
        name: String::from(path.file_name()?.to_str()?),
        is_symlink: file_type.is_symlink(),
        is_dir: is_dir,
        path: String::from(path.strip_prefix(HOME).unwrap().to_str()?),
        mtime: get_time(&metadata).unwrap(),
        file_type:
            if is_dir {
                String::from(DIRECTORY_FILE_TYPE)
            } else {
                String::from(guess_mime_type_opt(&path).unwrap_or(DEFAULT_MIME_TYPE.parse().unwrap()).type_().as_str())
            }
    })
}

fn readdir(path: &Path) -> Result<Vec<TemplateFile>, String> {
    match fs::read_dir(path) {
        Ok(files) => {
            let items: Vec<TemplateFile> =
            files.filter_map(|entry| {
                entry.ok().and_then(|e|
                    dir_entry_to_template_file(e)
                )
            }).collect();

            Ok(items)
        },
        Err(reason) => Err(reason.to_string())
    }
}

