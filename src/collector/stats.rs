use std::{
    borrow::Borrow,
    cmp,
    iter::FromIterator,
    ops::{Add, AddAssign},
};

#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub word_count: u32,
    pub paragraph_count: u32,
    pub longest_paragraph: u32,
}

impl Stats {
    pub fn push(&mut self, count: u32) {
        self.word_count += count;
        self.paragraph_count += 1;
        self.longest_paragraph = cmp::max(self.longest_paragraph, count);
    }

    pub fn average_paragraph(&self) -> u32 {
        match self.paragraph_count {
            0 => 0,
            x => self.word_count / x,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.word_count == 0
    }
}

impl<T: Borrow<Stats>> FromIterator<T> for Stats {
    fn from_iter<I: IntoIterator<Item = T>>(items: I) -> Self {
        let mut stats = Stats::default();
        for current in items {
            let current = current.borrow();
            stats.word_count += current.word_count;
            stats.paragraph_count += current.paragraph_count;
            stats.longest_paragraph = cmp::max(stats.longest_paragraph, current.longest_paragraph);
        }
        stats
    }
}

impl Add for Stats {
    type Output = Self;

    fn add(
        self,
        Stats {
            word_count,
            paragraph_count,
            longest_paragraph,
        }: Self,
    ) -> Self::Output {
        Stats {
            word_count: self.word_count + word_count,
            paragraph_count: self.paragraph_count + paragraph_count,
            longest_paragraph: self.longest_paragraph.max(longest_paragraph),
        }
    }
}

impl AddAssign for Stats {
    fn add_assign(
        &mut self,
        Stats {
            word_count,
            paragraph_count,
            longest_paragraph,
        }: Self,
    ) {
        self.word_count += word_count;
        self.paragraph_count += paragraph_count;
        self.longest_paragraph = self.longest_paragraph.max(longest_paragraph);
    }
}
