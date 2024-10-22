use core::fmt;

pub struct Heading<T>(pub T);

impl<T> fmt::Display for Heading<T>
where
    T: AsRef<str>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let heading = self.0.as_ref();
        if heading.len() <= 50 {
            write!(f, "{heading}")
        } else {
            write!(f, "{}...", &heading[..48])
        }
    }
}
