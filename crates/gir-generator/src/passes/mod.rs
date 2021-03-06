use crate::GIRGenerator;
use common::{bench, MutRc};
use gir_nodes::{Declaration, Module, ADT};
use std::rc::Rc;

pub(crate) use declare::FnSig;

mod declare;
mod fields;
mod generate;
mod import;
mod intrinsic_methods;
mod intrinsics;
mod methods;

impl GIRGenerator {
    pub(crate) fn run_passes(&mut self) {
        bench!("gir stage 1", {
            self.run_ast(Self::declare_adts);
            self.run_mod(Self::populate_intrinsics);
            self.run_mod(Self::import_stage_1);
            self.run_ast(Self::declare_iface_impls);
            self.run_ast(Self::declare_functions);
            self.run_mod(Self::populate_intrinsics_fn);
            self.validate_intrinsics();
            self.run_mod(Self::import_stage_2);
        });

        bench!("gir stage 2", {
            self.run_adt(Self::declare_methods);
            self.fill_impls();
            self.run_adt(Self::insert_adt_fields);
            self.run_adt(Self::constructor_setters);
            self.run_adt(Self::declare_lifecycle_methods);
            self.run_adt(Self::generate_lifecycle_methods);
        });

        bench!("gir generation", {
            self.run_dec(Self::generate);
            self.generate_impls();
        });
    }

    /// Execute a given module-scope pass.
    fn run_mod<T: FnMut(&mut Self, MutRc<Module>)>(&mut self, mut runner: T) {
        for module in self.modules_uncompiled.clone().into_iter() {
            self.switch_module(Rc::clone(&module));
            runner(self, module)
        }
    }

    /// Execute a given module-scope pass with AST data. Sets self.module to the
    /// module to be processed.
    fn run_ast<T: FnMut(&mut Self, &ast::Module)>(&mut self, mut runner: T) {
        for module in self.modules_uncompiled.clone().into_iter() {
            self.switch_module(Rc::clone(&module));
            let ast = module.borrow_mut().borrow_ast();
            runner(self, &ast.0);
            module.borrow_mut().return_ast(ast);
        }
    }

    /// Execute a given declaration-scope pass.
    fn run_dec<T: FnMut(&mut GIRGenerator, Declaration)>(&mut self, mut runner: T) {
        let declarations = self
            .modules_uncompiled
            .iter()
            .map(|module| {
                (
                    module
                        .borrow()
                        .declarations
                        .values()
                        .cloned()
                        .collect::<Vec<_>>(),
                    Rc::clone(module),
                )
            })
            .collect::<Vec<_>>();

        for (declarations, module) in declarations.into_iter() {
            self.switch_module(module);
            for decl in declarations {
                self.set_context(&decl.type_parameters());
                runner(self, decl)
            }
        }
    }

    fn run_adt<T: FnMut(&mut GIRGenerator, &MutRc<ADT>)>(&mut self, mut runner: T) {
        self.run_dec(|this, dec| {
            if let Declaration::Adt(adt) = &dec {
                runner(this, adt)
            }
        })
    }
}
