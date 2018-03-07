use glob::Paths;
use glob::glob as deglob;
use std::env::{self, Args};
use std::path::PathBuf;
use std::iter::Skip;

pub struct PathProvider {
    args: Skip<Args>,
    glob: Option<Paths>,
}

impl PathProvider {
    pub fn new() -> Self {
        Self {
            args: env::args().skip(1),
            glob: None
        }
    }

    fn next_globbed_path(&mut self) -> Option<PathBuf> {
        let candidate = self.glob.as_mut().and_then(|glob| glob.next());
        if let Some(Ok(path)) = candidate {
            return Some(path);
        }

        self.glob = None;
        None
    }
}

impl Iterator for PathProvider {
    type Item = PathBuf;

    fn next(&mut self) -> Option<Self::Item> {
        match self.next_globbed_path() {

            // No globbed paths available.
            None => match self.args.next() {
                None => None,
                Some(arg) => if arg.contains(glob_char) {
                    self.glob = deglob(&arg).ok();
                    self.next()
                } else {
                    Some(arg.into())
                },
            },

            // Return globbed path.
            path => path,
            
        }
    }
}

fn glob_char(c: char) -> bool {
    match c {
        '*' | '?' | '[' => true,
        _ => false,
    }
}
