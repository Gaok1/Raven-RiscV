use crate::ui::app::Lang;
use std::ops::Index;

pub struct T {
    pub en: &'static str,
    pub pt: &'static str,
}

impl T {
    pub const fn new(en: &'static str, pt: &'static str) -> Self {
        Self { en, pt }
    }
    pub fn get(&self, lang: Lang) -> &'static str {
        match lang {
            Lang::EN => self.en,
            Lang::PT => self.pt,
        }
    }
}

impl Index<Lang> for T {
    type Output = str;
    fn index(&self, index: Lang) -> &Self::Output {
        match index {
            Lang::EN => self.en,
            Lang::PT => self.pt,
        }
    }
}

