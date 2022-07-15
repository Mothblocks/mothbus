use std::{fmt::Debug, ops::Deref};

#[derive(Clone)]
pub struct HideDebug<T>(pub T);

impl<T> Debug for HideDebug<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(std::any::type_name::<T>())
    }
}

impl<T> Deref for HideDebug<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
