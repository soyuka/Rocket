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

use rocket::request::Request;
use rocket::response::{NamedFile, Failure};
use rocket_contrib::Template;
use std::collections::HashMap;
use std::path::{PathBuf, Path};

mod directory_path;
mod list;
pub use directory_path::DirectoryPath;
pub use list::list;

static HOME: &str = "/home/soyuka";

#[get("/css/<file..>")]
fn css(file: PathBuf) -> Option<NamedFile> {
    NamedFile::open(Path::new("css/").join(file)).ok()
}

#[get("/")]
fn index() -> Result<Template, Failure> {
    list(DirectoryPath::from_str(HOME), HOME)
}

#[get("/<path..>", rank = 2)]
fn dir(path: DirectoryPath) -> Result<Result<Template, Failure>, Option<NamedFile>> {
    let path = DirectoryPath::new(Path::new(HOME).join(path));

    if path.is_dir() {
        Ok(list(path, HOME))
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
