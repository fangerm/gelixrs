use std::{
    fmt,
    fmt::{Display, Formatter},
    hash::{Hash, Hasher},
    rc::Rc,
};

use crate::{declaration::LocalVariable, Function, ADT};
use common::MutRc;
use enum_methods::{EnumAsGetters, EnumIntoGetters, EnumIsA};
use gir_ir_adapter::IRClosure;
use smol_str::SmolStr;
use std::cell::Cell;

pub type TypeArguments = Vec<Type>;
pub type TypeParameters = Vec<TypeParameter>;

/// A type in GIR.
/// This *can* include type arguments for declarations,
/// but does not have to - types can be unresolved.
/// Type parameters are a separate type, monomorthised later in GIR.
#[derive(Debug, Clone, EnumAsGetters, EnumIsA, EnumIntoGetters)]
pub enum Type {
    /// Any type that can cast to anything; used by
    /// control flow branching away to allow phi usage with them
    Any,
    /// None singleton type used for expressions that do not produce a value
    None,
    /// Type of the `null` literal, cast to Nullable as appropriate
    Null,
    /// Simple boolean/i1 type.
    Bool,

    /// Signed integer types from 8 to 64 bit width.
    I8,
    I16,
    I32,
    I64,

    /// Unsigned integer types from 8 to 64 bit width.
    U8,
    U16,
    U32,
    U64,

    /// Floating-point numbers with 32 and 64 bit width.
    F32,
    F64,

    /// A function instance. This is a function itself, not a signature.
    Function(Instance<Function>),
    /// A closure signature.
    Closure(Rc<ClosureType>),
    /// The first parameter on a closure function
    ClosureCaptured(Rc<Vec<Rc<LocalVariable>>>),

    /// An ADT.
    Adt(Instance<ADT>),

    /// A nullable ADT that requires null checks before being usable.
    Nullable(Box<Type>),

    /// A raw pointer of a type.
    /// Can only be interacted with using special
    /// intrinsic functions; here for FFI and unsafe
    /// memory operations
    RawPtr(Box<Type>),

    /// An unresolved type parameter, resolved at IR.
    Variable(TypeVariable),
    /// A type itself. This is used for static fields,
    /// currently only enum cases.
    Type(Box<Type>),
}

impl Type {
    /// Compares equality between types.
    /// [strict] decides if Type::Any always equals
    /// other types or not.
    pub fn equal(&self, other: &Self, strict: bool) -> bool {
        match (self, other) {
            (Self::Any, _) | (_, Self::Any) => !strict,

            (Self::Function(f), Self::Function(o)) => f == o,
            (Self::Closure(f), Self::Closure(o)) => f == o,
            (Self::Adt(v), Self::Adt(o)) => v == o,
            (Self::Nullable(v), Self::Nullable(o)) => v == o,
            (Self::Type(v), Self::Type(o)) => v == o,
            (Self::Variable(i), Self::Variable(o)) => i.index == o.index,
            (Self::RawPtr(p), Self::RawPtr(o)) => p == o,

            _ => std::mem::discriminant(self) == std::mem::discriminant(other),
        }
    }

    /// Returns type arguments of this type, ifapplicable.
    pub fn type_args(&self) -> Option<Rc<TypeArguments>> {
        match self {
            Self::Function(inst) => Some(&inst.args).cloned(),
            Self::Adt(inst) => Some(&inst.args).cloned(),
            Self::Type(ty) | Self::RawPtr(ty) | Self::Nullable(ty) => ty.type_args(),
            Self::Variable(TypeVariable {
                bound: TypeParameterBound::Interface(iface),
                ..
            }) => iface.type_args(),
            _ => None,
        }
    }

    /// Returns type parameters of this type's prototype, if applicable.
    pub fn type_params(&self) -> Option<Rc<TypeParameters>> {
        Some(match self {
            Self::Function(inst) => Rc::clone(&inst.ty.borrow().type_parameters),
            Self::Adt(inst) => Rc::clone(&inst.ty.borrow().type_parameters),
            Self::Type(ty)
            | Self::RawPtr(ty)
            | Self::Nullable(ty)
            | Self::Variable(TypeVariable {
                bound: TypeParameterBound::Interface(ty),
                ..
            }) => return ty.type_params(),
            _ => return None,
        })
    }

    /// Sets type arguments of this type, if applicable.
    /// Returns success.
    pub fn set_type_args(&mut self, args: Rc<TypeArguments>) -> bool {
        match self {
            Self::Function(inst) => {
                inst.args = args;
                true
            }
            Self::Adt(inst) => {
                inst.args = args;
                true
            }
            Self::Type(ty) | Self::RawPtr(ty) | Self::Nullable(ty) => ty.set_type_args(args),
            _ => false,
        }
    }

    pub fn module(&self) -> Option<SmolStr> {
        match self {
            Type::Adt(a) => Some(a.ty.borrow().module.borrow().path.index(0).unwrap().clone()),
            _ => None,
        }
    }

    /// A list of all primitive types that are not defined in any gelix code,
    /// but are instead indirectly globally defined.
    pub fn primitives() -> [Type; 13] {
        [
            Type::Any,
            Type::None,
            Type::Bool,
            Type::I8,
            Type::I16,
            Type::I32,
            Type::I64,
            Type::U8,
            Type::U16,
            Type::U32,
            Type::U64,
            Type::F32,
            Type::F64,
        ]
    }

    /// Is this a primitive?
    pub fn is_primitive(&self) -> bool {
        self.is_none() || self.is_number()
    }

    /// Is this type a number?
    pub fn is_number(&self) -> bool {
        self.is_int() || self.is_float() || self.is_var_with_marker(Bound::Number)
    }

    /// Is this type an integer?
    pub fn is_int(&self) -> bool {
        self.is_signed_int()
            || self.is_unsigned_int()
            || self.is_bool()
            || self.is_var_with_marker(Bound::Integer)
    }

    /// Is this type a signed integer?
    pub fn is_signed_int(&self) -> bool {
        matches!(self, Type::I8 | Type::I16 | Type::I32 | Type::I64)
            || self.is_var_with_marker(Bound::SignedInt)
    }

    /// Is this type an unsigned integer?
    pub fn is_unsigned_int(&self) -> bool {
        matches!(self, Type::U8 | Type::U16 | Type::U32 | Type::U64)
            || self.is_var_with_marker(Bound::UnsignedInt)
    }

    /// Is this type a floating-point number?
    pub fn is_float(&self) -> bool {
        matches!(self, Type::F32 | Type::F64) || self.is_var_with_marker(Bound::Float)
    }

    /// Can this type be assigned to variables?
    /// True for everything but static ADTs, functions and null singleton.
    pub fn is_assignable(&self) -> bool {
        !self.is_function() && !self.is_type() && !self.is_null()
    }

    /// Can this type be called?
    /// True for functions and closures.
    pub fn is_callable(&self) -> bool {
        self.is_function() || self.is_closure()
    }

    /// Is this type a reference ADT?
    pub fn is_ref_adt(&self) -> bool {
        if let Type::Adt(inst) | Type::Nullable(box Type::Adt(inst)) = self {
            inst.ty.borrow().type_kind == TypeKind::Reference
        } else {
            false
        }
    }

    pub fn is_var_with_marker(&self, marker: Bound) -> bool {
        if let Type::Variable(var) = self {
            if let TypeParameterBound::Bound(bound) = &var.bound {
                marker == *bound
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Is `self` the nullable variant of `other`?
    pub fn is_nullable_of(&self, other: &Type) -> bool {
        if let Type::Nullable(inner) = self {
            other.equal(inner, false)
        } else {
            false
        }
    }

    /// Try turning this type into an ADT,
    /// if it contains one.
    pub fn try_adt(&self) -> Option<&Instance<ADT>> {
        match self {
            Type::Adt(adt) => Some(adt),
            _ => None,
        }
    }

    /// Try turning this type into an ADT or nullable,
    /// if it contains one.
    pub fn try_adt_nullable(&self) -> Option<&Instance<ADT>> {
        match self {
            Type::Adt(adt) => Some(adt),
            Type::Nullable(nil) => nil.try_adt_nullable(),
            _ => None,
        }
    }

    pub fn type_or_none(self) -> Option<Type> {
        match self {
            Type::None | Type::Any => None,
            _ => Some(self),
        }
    }

    pub fn resolve(&self, args: &Rc<TypeArguments>) -> Type {
        // Start by replacing any type variables with their concrete type
        let mut ty = match self {
            Type::Variable(var) if var.index < args.len() => args[var.index].clone(),
            Type::RawPtr(box Type::Variable(var)) if var.index < args.len() => {
                Type::RawPtr(box args[var.index].clone())
            }
            Type::Nullable(box Type::Variable(var)) if var.index < args.len() => {
                Type::Nullable(box args[var.index].clone())
            }
            _ => self.clone(),
        };

        // Resolve any type args on itself if present,
        // for example resolving SomeAdt[T] to SomeAdt[ActualType]
        if let Some(a) = ty.type_args() {
            let new = Rc::new(a.iter().map(|a| a.resolve(args)).collect::<Vec<_>>());
            ty.set_type_args(new);
        }

        // If the type has empty type args but needs some, attach given ones
        // Done after arg resolution to prevent resolving given ones when that is not needed
        if self.type_args().map(|a| a.is_empty()).unwrap_or(false)
            && self.type_params().map(|a| !a.is_empty()).unwrap_or(false)
        {
            ty.set_type_args(Rc::clone(args));
        }

        ty
    }
}

impl PartialEq for Type {
    fn eq(&self, other: &Self) -> bool {
        self.equal(other, true)
    }
}

impl Eq for Type {}

impl Hash for Type {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Self::Function(v) => v.ty.borrow().name.hash(state),

            Self::Adt(v) => v.ty.borrow().name.hash(state),

            Self::Type(v) | Self::RawPtr(v) | Self::Nullable(v) => v.hash(state),

            Self::Closure(cls) => {
                for param in &cls.parameters {
                    param.hash(state);
                }
                cls.ret_type.hash(state)
            }

            Self::ClosureCaptured(cap) => cap.iter().for_each(|i| i.ty.hash(state)),

            Self::Variable(var) => var.index.hash(state),

            _ => std::mem::discriminant(self).hash(state),
        }
    }
}

impl Default for Type {
    fn default() -> Self {
        Type::None
    }
}

impl Display for Type {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Type::Function(_) => write!(f, "<function>"),
            Type::Closure(closure) => write!(f, "{}", closure),
            Type::Adt(adt) => write!(f, "{}", adt),
            Type::Nullable(adt) => write!(f, "{}?", adt),
            Type::RawPtr(inner) => write!(f, "*{}", inner),
            Type::Variable(var) => write!(f, "{}: {}", var.name, var.bound),
            Type::Type(ty) => match **ty {
                Type::Function(_) => write!(f, "<function>"),
                Type::Closure(_) => write!(f, "<closure>"),
                Type::Adt(_) => write!(f, "<ADT>"),
                Type::Nullable(_) => write!(f, "<nullable ADT>"),
                _ => write!(f, "<{:?}>", self),
            },
            _ => write!(f, "{:?}", self),
        }
    }
}

/// An "instance" of a declaration, with type arguments.
/// Arguments can be absent from the type if it is to be used
/// generically; should not be absent in final GIR produced.
#[derive(Debug)]
pub struct Instance<T> {
    pub ty: MutRc<T>,
    args: Rc<TypeArguments>,
}

impl<T> Instance<T> {
    /// Create a new instance. Will register with inner type.
    pub fn new(ty: MutRc<T>, args: Rc<TypeArguments>) -> Instance<T> {
        Instance { ty, args }
    }

    /// Create a new instance with no type arguments.
    pub fn new_(ty: MutRc<T>) -> Instance<T> {
        Instance {
            ty,
            args: Rc::new(vec![]),
        }
    }

    pub fn args(&self) -> &Rc<TypeArguments> {
        &self.args
    }

    pub fn set_args(&mut self, args: Rc<TypeArguments>) {
        self.args = args;
    }
}

impl Instance<ADT> {
    pub fn get_method(&self, name: &str) -> Instance<Function> {
        Instance::new(
            Rc::clone(self.ty.borrow().methods.get(name).unwrap()),
            Rc::clone(&self.args),
        )
    }

    pub fn try_get_method(&self, name: &str) -> Option<Instance<Function>> {
        Some(Instance::new(
            Rc::clone(self.ty.borrow().methods.get(name)?),
            Rc::clone(&self.args),
        ))
    }
}

impl Display for Instance<ADT> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}", self.ty.borrow().name)?;
        print_type_args(f, &self.args)
    }
}

pub fn print_type_args(f: &mut Formatter, args: &[Type]) -> fmt::Result {
    if !args.is_empty() {
        let mut args = args.iter();
        args.next().map(|arg| write!(f, "[{}", arg));
        for arg in args {
            write!(f, ", {}", arg)?;
        }
        write!(f, "]")?;
    }
    Ok(())
}

impl<T> Clone for Instance<T> {
    /// Clone this instance; does 2 Rc clones
    fn clone(&self) -> Self {
        Self {
            ty: Rc::clone(&self.ty),
            args: Rc::clone(&self.args),
        }
    }
}

impl<T> PartialEq for Instance<T> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.ty, &other.ty) && self.args == other.args
    }
}

impl<T> Eq for Instance<T> {}

impl<T: Hash> Hash for Instance<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.ty.borrow().hash(state);
        if !self.args.is_empty() {
            self.args.hash(state)
        }
    }
}

pub trait ToInstance<T> {
    fn to_inst(&self) -> Instance<T>;
    fn to_type(&self) -> Type;
}

impl ToInstance<ADT> for MutRc<ADT> {
    fn to_inst(&self) -> Instance<ADT> {
        Instance::new_(Rc::clone(self))
    }

    fn to_type(&self) -> Type {
        Type::Adt(self.to_inst())
    }
}

impl ToInstance<Function> for MutRc<Function> {
    fn to_inst(&self) -> Instance<Function> {
        Instance::new_(Rc::clone(self))
    }

    fn to_type(&self) -> Type {
        Type::Function(self.to_inst())
    }
}

/// Type parameter to be used when monomorphising.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TypeVariable {
    pub index: usize,
    pub name: SmolStr,
    pub bound: TypeParameterBound,
}

impl TypeVariable {
    pub fn from_param(param: &TypeParameter) -> TypeVariable {
        TypeVariable {
            index: param.index,
            name: param.name.clone(),
            bound: param.bound.clone(),
        }
    }
}

/// A closure signature.
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct ClosureType {
    pub parameters: Vec<Type>,
    pub ret_type: Type,
    pub ir: Cell<Option<IRClosure>>,
}

impl Display for ClosureType {
    fn fmt(&self, f: &mut Formatter) -> Result<(), fmt::Error> {
        write!(f, "(")?;
        if !self.parameters.is_empty() {
            let mut p_iter = self.parameters.iter();
            write!(f, "{}", p_iter.next().unwrap())?;
            for ty in p_iter {
                write!(f, ", {}", ty)?;
            }
        }
        write!(f, "): {}", self.ret_type)
    }
}

/// A single type parameter on a declaration.
#[derive(Debug, Clone)]
pub struct TypeParameter {
    /// Name of the parameter to use
    pub name: SmolStr,
    /// Index in list of parameters
    pub index: usize,
    /// The bound to use for arguments
    pub bound: TypeParameterBound,
}

/// Bound for a type parameter.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum TypeParameterBound {
    /// Bound on an interface; argument must implement it
    Interface(Box<Type>),
    /// Bound on some builtin bound marker
    Bound(Bound),
}

impl Default for TypeParameterBound {
    fn default() -> Self {
        TypeParameterBound::Bound(Bound::Unbounded)
    }
}

impl Display for TypeParameterBound {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            TypeParameterBound::Interface(iface) => write!(f, "{}", iface),
            TypeParameterBound::Bound(b) => write!(f, "{:?}", b),
        }
    }
}

/// A bound marker that is built into gelix.
/// See gelix docs for details on them.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Bound {
    Unbounded,
    Primitive,
    Number,
    Integer,
    SignedInt,
    UnsignedInt,
    Float,
    Adt,
    Nullable,
}

/// The kind a type can be - either a reference type,
/// or a value type. See gelix docs for more info and differences.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum TypeKind {
    Reference,
    Value,
}
