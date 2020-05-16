extern crate bindgen;
extern crate postgres_util;

use std::collections::HashSet;
use std::env;
use std::fs::{read_dir, File};
use std::io::Write;
use std::path::PathBuf;

fn main() {
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    let wrapper_h_path = out_path.join("wrapper.h");
    let bindings_path = out_path.join("bindings.rs");
    let postgres = postgres_util::postgres();
    let includedir = PathBuf::from(&postgres["INCLUDEDIR-SERVER"]);
    let mut headers: Vec<PathBuf> = Vec::new();
    walkdir(&includedir, &includedir, &mut headers);
    headers.sort();

    write_wrapper_h(&wrapper_h_path, &headers);

    let bindings = bindgen::Builder::default()
        .clang_arg(format!("-I{}", includedir.to_str().unwrap()))
        .header(wrapper_h_path.to_str().unwrap())
        .parse_callbacks(Box::new(parse_callbacks()))
        .derive_default(true)
        .rustfmt_bindings(true)
        .generate()
        .expect("Unable to generate bindings");

    bindings
        .write_to_file(bindings_path)
        .expect("Couldn't write bindings!");
}

fn include_line(header_path: &PathBuf) -> String {
    format!("#include \"{}\"\n", header_path.to_str().unwrap())
}

fn write_wrapper_h(wrapper_path: &PathBuf, headers: &Vec<PathBuf>) {
    let mut file = File::create(wrapper_path).unwrap();

    // write postgres.h first
    let postgres_h = PathBuf::from("postgres.h");
    file.write_all(include_line(&postgres_h).as_bytes())
        .unwrap();

    // write the rest
    for header_path in headers {
        file.write_all(include_line(header_path).as_bytes())
            .unwrap();
    }
}

fn walkdir(base: &PathBuf, dir: &PathBuf, files: &mut Vec<PathBuf>) {
    if dir.is_dir() {
        for entry in read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let relpath = path.strip_prefix(base).unwrap().to_path_buf();
            if !include_filter(&relpath) {
                continue;
            }
            if path.is_dir() {
                walkdir(base, &path, files);
            } else {
                files.push(relpath);
            }
        }
    }
}

// Ignore paths that can't be processed. Note: file may still be
// included indirectly if another header includes it. TODO:
// investigate these cases to try to reduce this list.
fn include_filter(path: &PathBuf) -> bool {
    let ignore = vec![
        // files that can't be processed
        PathBuf::from("be-gssapi-common.h"), // v12, v13
        PathBuf::from("rmgrlist.h"), // all
        PathBuf::from("pg_rusage.h"), // all
        PathBuf::from("kwlist.h"), // all
        PathBuf::from("gram.h"), // all
        PathBuf::from("wait.h"), // v13
        PathBuf::from("hashutils.h"), // v13
        PathBuf::from("jsonfuncs.h"), // v13
        PathBuf::from("cmdtaglist.h"), // v13
        PathBuf::from("pg_cast_d.h"), // v10
        // directories that can't be processed
        PathBuf::from("libstemmer"), // all
        PathBuf::from("fe_utils"), // all
        PathBuf::from("regex"), // all
        PathBuf::from("jit"), // v13
        PathBuf::from("common"), // all
    ];
    for item in ignore {
        if path.ends_with(item) {
            return false;
        }
    }
    if path.starts_with(PathBuf::from("port")) && !path.ends_with(PathBuf::from("port.h")) {
        return false;
    }

    return true;
}

#[derive(Debug)]
struct PkgParseCallbacks {
    ignore_macros: HashSet<String>,
}

// Instantiate parser callbacks with list of macros to ignore. These
// macros come from math.h, and conflict with enum variants on some
// platforms. See https://github.com/rust-lang/rust-bindgen/issues/687
// for more information.
fn parse_callbacks() -> PkgParseCallbacks {
    let blacklist: Vec<String> = vec![
        "FP_INFINITE".into(),
        "FP_NAN".into(),
        "FP_NORMAL".into(),
        "FP_SUBNORMAL".into(),
        "FP_ZERO".into(),
        "IPPORT_RESERVED".into(),
    ];
    PkgParseCallbacks {
        ignore_macros: blacklist.into_iter().collect(),
    }
}

impl bindgen::callbacks::ParseCallbacks for PkgParseCallbacks {
    // ignore macros from given list
    fn will_parse_macro(&self, name: &str) -> bindgen::callbacks::MacroParsingBehavior {
        if self.ignore_macros.contains(name) {
            bindgen::callbacks::MacroParsingBehavior::Ignore
        } else {
            bindgen::callbacks::MacroParsingBehavior::Default
        }
    }

    // Tell cargo to invalidate the built crate whenever any of the
    // included header files changed. Copied from
    // bindgen::CargoCallbacks.
    fn include_file(&self, filename: &str) {
        println!("cargo:rerun-if-changed={}", filename);
    }
}
