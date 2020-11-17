use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use crate::gir::{
    generator::GIRGenerator,
    nodes::declaration::{Declaration, Field, ADT},
    MutRc,
};

impl GIRGenerator {
    pub fn insert_adt_fields(&mut self, decl: Declaration) {
        match decl {
            Declaration::Adt(adt) if adt.borrow().ty.has_members() => self.fill_adt(adt),
            _ => (),
        }
    }

    fn fill_adt(&mut self, adt: MutRc<ADT>) {
        self.build_adt(&adt);
        self.check_duplicate(&adt);
    }

    /// This function will fill the ADT with its members.
    fn build_adt(&mut self, adt: &MutRc<ADT>) {
        let ast = Rc::clone(&adt.borrow().ast);
        let ast = ast.borrow();

        for (index, field) in ast.members().unwrap().iter().enumerate() {
            let initializer = field.initializer.as_ref().map(|e| self.expression(e));
            let ty = eat!(
                self,
                initializer.as_ref().map_or_else(
                    || self.resolver.find_type(field.ty.as_ref().unwrap()),
                    |i| Ok(i.get_type()),
                )
            );

            if !ty.can_escape() {
                self.err(
                    &field.name,
                    "ADT field may not be a weak reference".to_string(),
                );
            }

            let member = Rc::new(Field {
                name: field.name.lexeme.clone(),
                mutable: field.mutable,
                ty,
                initializer: RefCell::new(initializer.map(Box::new)),
                index,
            });

            let existing_entry = adt
                .borrow_mut()
                .fields
                .insert(field.name.lexeme.clone(), Rc::clone(&member));
            if existing_entry.is_some() {
                self.err(
                    &field.name,
                    "Class member cannot be defined twice".to_string(),
                );
            }
        }
    }

    fn check_duplicate(&self, adt: &MutRc<ADT>) {
        let adt = adt.borrow();
        for (mem_name, _) in adt.fields.iter() {
            if adt.methods.contains_key(mem_name) {
                self.err(
                    &adt.ast.borrow().name,
                    format!(
                        "Cannot have member and method '{}' with same name.",
                        mem_name
                    ),
                );
            }
        }
    }
}