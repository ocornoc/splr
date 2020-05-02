/// Decision var selection
use {
    super::{AssignStack, VarHeapIF, VarOrderIF},
    crate::types::*,
    std::slice::Iter,
};

/// ```
/// let x: Lbool = var_assign!(self, lit.vi());
/// ```
macro_rules! var_assign {
    ($asg: expr, $var: expr) => {
        unsafe { *$asg.assign.get_unchecked($var) }
    };
}

/// API for var selection, depending on an internal heap.
pub trait VarSelectIF {
    /// force assignments
    fn force_select(&mut self, iterator: Iter<'_, usize>);
    /// select a new decision variable.
    fn select_var(&mut self) -> VarId;
    /// update the internal heap on var order.
    fn update_order(&mut self, v: VarId);
    /// rebuild the internal var_order
    fn rebuild_order(&mut self);
}

impl VarSelectIF for AssignStack {
    fn force_select(&mut self, iterator: Iter<'_, usize>) {
        for vi in iterator.rev() {
            self.temp_order.push(*vi);
        }
    }
    fn select_var(&mut self) -> VarId {
        while let Some(vi) = self.temp_order.pop() {
            if self.assign[vi].is_none() && !self.var[vi].is(Flag::ELIMINATED) {
                return vi;
            }
        }
        loop {
            let vi = self.get_heap_root();
            if var_assign!(self, vi).is_none() && !self.var[vi].is(Flag::ELIMINATED) {
                return vi;
            }
        }
    }
    fn update_order(&mut self, v: VarId) {
        self.update_heap(v);
    }
    fn rebuild_order(&mut self) {
        self.var_order.clear();
        for vi in 1..self.var.len() {
            if var_assign!(self, vi).is_none() && !self.var[vi].is(Flag::ELIMINATED) {
                self.insert_heap(vi);
            }
        }
    }
}
