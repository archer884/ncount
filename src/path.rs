use glob::glob as deglob;
use glob::Paths;
use std::path::PathBuf;
use std::slice::Iter;

pub struct PathProvider<'p, T: 'p> {
    paths: Iter<'p, T>,
    glob: Option<Paths>,
}

impl<'p, T: 'p> PathProvider<'p, T> {
    pub fn new(paths: &'p [T]) -> Self {
        Self {
            paths: paths.iter(),
            glob: None,
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

impl<'p, T: AsRef<str> + 'p> Iterator for PathProvider<'p, T> {
    type Item = PathBuf;

    fn next(&mut self) -> Option<Self::Item> {
        match self.next_globbed_path() {
            // No globbed paths available.
            None => match self.paths.next() {
                None => None,
                Some(path) => {
                    let path = path.as_ref();
                    if path.contains(glob_char) {
                        self.glob = deglob(path).ok();
                        self.next()
                    } else {
                        Some(path.into())
                    }
                }
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
