use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct Opt {
    paths: Vec<String>,
}

impl Opt {
    pub fn from_args() -> Opt {
        StructOpt::from_args()
    }

    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.paths
            .iter()
            .map(AsRef::as_ref)
            .once_if_empty_with(|| ".")
    }
}

/// An iterator adaptor providing a single default value if the inner source produces no items.
#[derive(Debug)]
struct OnceIfEmptyWith<I, F> {
    source: I,
    with: F,
    has_produced: bool,
}

trait IterExt: Iterator + Sized {
    fn once_if_empty_with<F>(self, with: F) -> OnceIfEmptyWith<Self, F>
    where
        F: FnMut() -> Self::Item,
    {
        OnceIfEmptyWith {
            source: self,
            with,
            has_produced: false,
        }
    }
}

impl<T: Iterator> IterExt for T {}

impl<I, F> Iterator for OnceIfEmptyWith<I, F>
where
    I: Iterator,
    F: FnMut() -> I::Item,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        match self.source.next() {
            None if self.has_produced => None,
            None => {
                self.has_produced = true;
                Some((self.with)())
            }
            Some(item) => {
                self.has_produced = true;
                Some(item)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::IterExt;
    use std::iter;

    #[test]
    fn empty_source_iter() {
        let mut items = iter::empty().once_if_empty_with(|| 1);
        assert_eq!(Some(1), items.next());
        assert_eq!(None, items.next());
    }

    #[test]
    fn non_empty_source_iter() {
        let mut items = vec![1].into_iter().once_if_empty_with(|| 2);
        assert_eq!(Some(1), items.next());
        assert_eq!(None, items.next());
    }
}
