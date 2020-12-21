use smol_str::SmolStr;
use std::{
    fmt::{Display, Error, Formatter},
    rc::Rc,
};

/// The path of a module in the context of a gelix program.
/// For example, the file 'std/collections/array.gel' would have `["std", "collections", "array"]` here.
pub type ModulePath = Rc<ModPath>;

#[derive(Clone, Debug, Default, PartialOrd, PartialEq, Eq, Hash)]
pub struct ModPath(Vec<SmolStr>);

impl Display for ModPath {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        write!(
            f,
            "{}",
            self.0
                .iter()
                .map(|rc| rc.as_ref())
                .collect::<Vec<&str>>()
                .join("/")
        )
    }
}
