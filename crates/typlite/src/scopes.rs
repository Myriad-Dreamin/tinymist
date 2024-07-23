//! Variable scopes.

use super::Result;
use std::{borrow::Cow, collections::HashMap};

/// A single scope.
#[derive(Debug, Clone)]
pub struct Scope<T> {
    map: HashMap<String, T>,
}

impl<T> Default for Scope<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Scope<T> {
    /// Create a new, empty scope.
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    /// Define a variable in this scope.
    pub fn define(&mut self, name: String, val: T) {
        self.map.insert(name, val);
    }

    /// Try to access a variable immutably.
    pub fn get(&self, var: &str) -> Option<&T> {
        self.map.get(var)
    }

    /// Try to access a variable mutably.
    pub fn get_mut(&mut self, var: &str) -> Option<&mut T> {
        self.map.get_mut(var)
    }
}

/// A stack of scopes.
#[derive(Debug, Default, Clone)]
pub struct Scopes<T> {
    /// The active scope.
    pub top: Scope<T>,
    /// The stack of lower scopes.
    pub scopes: Vec<Scope<T>>,
}

impl<T> Scopes<T> {
    /// Create a new, empty hierarchy of scopes.
    pub fn new() -> Self {
        Self {
            top: Scope::new(),
            scopes: vec![],
        }
    }

    /// Enter a new scope.
    pub fn enter(&mut self) {
        self.scopes.push(std::mem::take(&mut self.top));
    }

    /// Exit the topmost scope.
    ///
    /// This panics if no scope was entered.
    pub fn exit(&mut self) {
        self.top = self.scopes.pop().expect("no pushed scope");
    }

    /// Try to access a variable immutably.
    pub fn get(&self, var: &str) -> Result<&T> {
        std::iter::once(&self.top)
            .chain(self.scopes.iter().rev())
            .find_map(|scope| scope.get(var))
            .ok_or_else(|| unknown_variable(var))
    }

    /// Try to access a variable immutably in math.
    pub fn get_in_math(&self, var: &str) -> Result<&T> {
        std::iter::once(&self.top)
            .chain(self.scopes.iter().rev())
            .find_map(|scope| scope.get(var))
            .ok_or_else(|| unknown_variable(var))
    }

    /// Try to access a variable mutably.
    pub fn get_mut(&mut self, var: &str) -> Result<&mut T> {
        std::iter::once(&mut self.top)
            .chain(&mut self.scopes.iter_mut().rev())
            .find_map(|scope| scope.get_mut(var))
            .ok_or_else(|| unknown_variable(var))
    }

    /// Define a variable in the current scope.
    pub fn define(&mut self, arg: &str, v: impl Into<T>) {
        self.top.define(arg.to_string(), v.into());
    }
}

/// The error message when a variable is not found.
fn unknown_variable(var: &str) -> Cow<'static, str> {
    Cow::Owned(format!("unknown variable: {var}"))
}
