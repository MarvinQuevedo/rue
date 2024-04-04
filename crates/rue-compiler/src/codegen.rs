use std::collections::HashMap;

use clvmr::{Allocator, NodePtr};
use indexmap::IndexSet;

use crate::{
    database::{Database, ScopeId, SymbolId},
    symbol::Symbol,
    Value,
};

pub fn codegen(allocator: &mut Allocator, db: &mut Database, main: SymbolId) -> NodePtr {
    Codegen::new(db, allocator).gen_main(main)
}

struct Codegen<'a> {
    db: &'a mut Database,
    allocator: &'a mut Allocator,
    captures: HashMap<ScopeId, IndexSet<SymbolId>>,
}

impl<'a> Codegen<'a> {
    pub fn new(db: &'a mut Database, allocator: &'a mut Allocator) -> Self {
        Self {
            db,
            allocator,
            captures: HashMap::new(),
        }
    }

    fn compute_captures(&mut self, scope_id: ScopeId) {
        if self.captures.contains_key(&scope_id) {
            return;
        }

        self.captures.insert(scope_id, IndexSet::new());

        for used_id in self.db.scope(scope_id).used_symbols().clone() {
            if !self.db.scope(scope_id).definitions().contains(&used_id) {
                self.captures
                    .get_mut(&scope_id)
                    .expect("cannot capture from unknown scope")
                    .insert(used_id);
            }

            let Symbol::Function {
                scope_id: function_scope_id,
                ..
            } = self.db.symbol(used_id)
            else {
                continue;
            };

            let function_scope_id = *function_scope_id;

            self.compute_captures(function_scope_id);

            if !self.db.scope(scope_id).definitions().contains(&used_id) {
                let new_captures = self.captures[&function_scope_id].clone();
                self.captures
                    .get_mut(&scope_id)
                    .expect("cannot capture from unknown scope")
                    .extend(new_captures);
            }
        }
    }

    pub fn gen_main(&mut self, main: SymbolId) -> NodePtr {
        let (scope_id, value) = {
            let Symbol::Function {
                scope_id, value, ..
            } = self.db.symbol(main)
            else {
                unreachable!();
            };
            (*scope_id, value.clone())
        };

        self.compute_captures(scope_id);

        let body = self.gen_value(scope_id, value.clone());
        let quoted_body = self.quote(body);
        let rest = self.allocator.one();
        let a = self
            .allocator
            .new_small_number(2)
            .expect("could not allocate `a`");

        let mut args = Vec::new();

        for symbol_id in self.captures[&scope_id].clone() {
            args.push(self.gen_symbol(symbol_id));
        }

        let arg_list = self.runtime_list(&args, rest);

        self.list(&[a, quoted_body, arg_list])
    }

    fn gen_symbol(&mut self, symbol_id: SymbolId) -> NodePtr {
        match self.db.symbol(symbol_id) {
            Symbol::Function {
                scope_id, value, ..
            } => self.gen_value(
                *scope_id,
                Value::Function {
                    scope_id: *scope_id,
                    value: Box::new(value.clone()),
                },
            ),
            Symbol::Parameter { .. } => todo!(),
        }
    }

    fn gen_value(&mut self, scope_id: ScopeId, value: Value) -> NodePtr {
        match value {
            Value::Atom(atom) => self.gen_atom(atom),
            Value::List(list) => self.gen_list(scope_id, list),
            Value::Reference(symbol_id) => self.gen_reference(scope_id, symbol_id),
            Value::FunctionCall { callee, args } => self.gen_function_call(scope_id, *callee, args),
            Value::Function { scope_id, value } => self.gen_function(scope_id, *value),
            Value::Add(operands) => self.gen_add(scope_id, operands),
            Value::Subtract(operands) => self.gen_subtract(scope_id, operands),
            Value::Multiply(operands) => self.gen_multiply(scope_id, operands),
            Value::Divide(lhs, rhs) => self.gen_divide(scope_id, *lhs, *rhs),
            Value::Remainder(lhs, rhs) => self.gen_remainder(scope_id, *lhs, *rhs),
            Value::LessThan(lhs, rhs) => self.gen_lt(scope_id, *lhs, *rhs),
            Value::GreaterThan(lhs, rhs) => self.gen_gt(scope_id, *lhs, *rhs),
            Value::LessThanEquals(lhs, rhs) => self.gen_lteq(scope_id, *rhs, *lhs),
            Value::GreaterThanEquals(lhs, rhs) => self.gen_gteq(scope_id, *rhs, *lhs),
            Value::Equals(lhs, rhs) => self.gen_eq(scope_id, *lhs, *rhs),
            Value::NotEquals(lhs, rhs) => self.gen_neq(scope_id, *rhs, *lhs),
            Value::Not(value) => self.gen_not(scope_id, *value),
            Value::If {
                condition,
                then_block,
                else_block,
            } => self.gen_if(scope_id, *condition, *then_block, *else_block),
        }
    }

    fn gen_list(&mut self, scope_id: ScopeId, items: Vec<Value>) -> NodePtr {
        let mut args = Vec::new();
        for item in items {
            args.push(self.gen_value(scope_id, item));
        }
        self.runtime_list(&args, NodePtr::NIL)
    }

    fn gen_reference(&mut self, scope_id: ScopeId, symbol_id: SymbolId) -> NodePtr {
        if let Symbol::Function {
            scope_id: function_scope_id,
            value: _,
            ..
        } = self.db.symbol(symbol_id).clone()
        {
            let body = self.gen_path(scope_id, symbol_id);

            let mut captures = Vec::new();

            for symbol_id in self.captures[&function_scope_id].clone() {
                captures.push(self.gen_path(scope_id, symbol_id));
            }

            return self.gen_closure_wrapper(body, &captures);
        }

        self.gen_path(scope_id, symbol_id)
    }

    fn gen_closure_wrapper(&mut self, body: NodePtr, captures: &[NodePtr]) -> NodePtr {
        let q = self.allocator.one();
        let one = q;
        let a = self
            .allocator
            .new_small_number(2)
            .expect("could not allocate `a`");

        let runtime_a = self.quote(a);
        let runtime_quoted_body = self.runtime_quote(body);

        let mut args = Vec::new();

        for &capture in captures {
            let runtime_quoted_arg = self.runtime_quote(capture);
            args.push(runtime_quoted_arg);
        }

        let quoted_one = self.quote(one);
        let runtime_args = self.runtime_runtime_list(&args, quoted_one);

        self.runtime_list(
            &[runtime_a, runtime_quoted_body, runtime_args],
            NodePtr::NIL,
        )
    }

    fn gen_function_call(
        &mut self,
        scope_id: ScopeId,
        callee: Value,
        arg_values: Vec<Value>,
    ) -> NodePtr {
        let a = self
            .allocator
            .new_small_number(2)
            .expect("could not allocate `a`");

        let mut args = Vec::new();

        let callee = if let Value::Reference(symbol_id) = callee {
            if let Symbol::Function {
                scope_id: callee_scope_id,
                ..
            } = self.db.symbol(symbol_id)
            {
                for symbol_id in self.captures[&callee_scope_id].clone() {
                    args.push(self.gen_path(scope_id, symbol_id));
                }
                self.gen_path(scope_id, symbol_id)
            } else {
                self.gen_value(scope_id, callee)
            }
        } else {
            self.gen_value(scope_id, callee)
        };

        for arg_value in arg_values {
            args.push(self.gen_value(scope_id, arg_value));
        }
        let arg_list = self.runtime_list(&args, NodePtr::NIL);

        self.list(&[a, callee, arg_list])
    }

    fn gen_function(&mut self, scope_id: ScopeId, value: Value) -> NodePtr {
        let body = self.gen_value(scope_id, value);
        self.quote(body)
    }

    fn gen_add(&mut self, scope_id: ScopeId, operands: Vec<Value>) -> NodePtr {
        let plus = self
            .allocator
            .new_small_number(16)
            .expect("could not allocate `+`");

        let mut args = vec![plus];
        for operand in operands {
            args.push(self.gen_value(scope_id, operand));
        }
        self.list(&args)
    }

    fn gen_subtract(&mut self, scope_id: ScopeId, operands: Vec<Value>) -> NodePtr {
        let minus = self
            .allocator
            .new_small_number(17)
            .expect("could not allocate `-`");

        let mut args = vec![minus];
        for operand in operands {
            args.push(self.gen_value(scope_id, operand));
        }
        self.list(&args)
    }

    fn gen_multiply(&mut self, scope_id: ScopeId, operands: Vec<Value>) -> NodePtr {
        let star = self
            .allocator
            .new_small_number(18)
            .expect("could not allocate `*`");

        let mut args = vec![star];
        for operand in operands {
            args.push(self.gen_value(scope_id, operand));
        }
        self.list(&args)
    }

    fn gen_divide(&mut self, scope_id: ScopeId, lhs: Value, rhs: Value) -> NodePtr {
        let slash = self
            .allocator
            .new_small_number(19)
            .expect("could not allocate `/`");

        let lhs = self.gen_value(scope_id, lhs);
        let rhs = self.gen_value(scope_id, rhs);

        self.list(&[slash, lhs, rhs])
    }

    fn gen_remainder(&mut self, scope_id: ScopeId, lhs: Value, rhs: Value) -> NodePtr {
        let divmod = self
            .allocator
            .new_small_number(20)
            .expect("could not allocate `divmod`");
        let rest = self
            .allocator
            .new_small_number(6)
            .expect("could not allocate `r`");

        let lhs = self.gen_value(scope_id, lhs);
        let rhs = self.gen_value(scope_id, rhs);

        let divmod_list = self.list(&[divmod, lhs, rhs]);
        self.list(&[rest, divmod_list])
    }

    fn gen_lt(&mut self, scope_id: ScopeId, lhs: Value, rhs: Value) -> NodePtr {
        self.gen_gt(scope_id, rhs, lhs)
    }

    fn gen_gt(&mut self, scope_id: ScopeId, lhs: Value, rhs: Value) -> NodePtr {
        let gt = self
            .allocator
            .new_small_number(21)
            .expect("could not allocate `>`");

        let mut args = vec![gt];
        args.push(self.gen_value(scope_id, lhs));
        args.push(self.gen_value(scope_id, rhs));
        self.list(&args)
    }

    fn gen_lteq(&mut self, scope_id: ScopeId, lhs: Value, rhs: Value) -> NodePtr {
        let not = self
            .allocator
            .new_small_number(32)
            .expect("could not allocate `not`");
        let gt = self
            .allocator
            .new_small_number(21)
            .expect("could not allocate `>`");

        let lhs = self.gen_value(scope_id, lhs);
        let rhs = self.gen_value(scope_id, rhs);
        let gt_list = self.list(&[gt, lhs, rhs]);

        self.list(&[not, gt_list])
    }

    fn gen_gteq(&mut self, scope_id: ScopeId, lhs: Value, rhs: Value) -> NodePtr {
        let any = self
            .allocator
            .new_small_number(33)
            .expect("could not allocate `any`");
        let eq = self
            .allocator
            .new_small_number(9)
            .expect("could not allocate `=`");
        let gt = self
            .allocator
            .new_small_number(21)
            .expect("could not allocate `>`");

        let lhs = self.gen_value(scope_id, lhs);
        let rhs = self.gen_value(scope_id, rhs);
        let operands = self.list(&[lhs, rhs]);

        let eq_list = self
            .allocator
            .new_pair(eq, operands)
            .expect("could not allocate eq list");
        let gt_list = self
            .allocator
            .new_pair(gt, operands)
            .expect("could not allocate gt list");

        self.list(&[any, gt_list, eq_list])
    }

    fn gen_eq(&mut self, scope_id: ScopeId, lhs: Value, rhs: Value) -> NodePtr {
        let eq = self
            .allocator
            .new_small_number(9)
            .expect("could not allocate `=`");

        let mut args = vec![eq];
        args.push(self.gen_value(scope_id, lhs));
        args.push(self.gen_value(scope_id, rhs));
        self.list(&args)
    }

    fn gen_neq(&mut self, scope_id: ScopeId, lhs: Value, rhs: Value) -> NodePtr {
        let eq = self
            .allocator
            .new_small_number(9)
            .expect("could not allocate `=`");
        let not = self
            .allocator
            .new_small_number(32)
            .expect("could not allocate `not`");

        let mut args = vec![eq];
        args.push(self.gen_value(scope_id, lhs));
        args.push(self.gen_value(scope_id, rhs));
        let eq_list = self.list(&args);

        self.list(&[not, eq_list])
    }

    fn gen_not(&mut self, scope_id: ScopeId, value: Value) -> NodePtr {
        let not = self
            .allocator
            .new_small_number(32)
            .expect("could not allocate `not`");

        let value = self.gen_value(scope_id, value);
        self.list(&[not, value])
    }

    fn gen_if(
        &mut self,
        scope_id: ScopeId,
        condition: Value,
        then_block: Value,
        else_block: Value,
    ) -> NodePtr {
        let a = self
            .allocator
            .new_small_number(2)
            .expect("could not allocate `a`");
        let i = self
            .allocator
            .new_small_number(3)
            .expect("could not allocate `i`");

        let all_env = self.allocator.one();

        let condition = self.gen_value(scope_id, condition);
        let then_block = self.gen_value(scope_id, then_block);
        let else_block = self.gen_value(scope_id, else_block);

        let then_block = self.quote(then_block);
        let else_block = self.quote(else_block);

        let conditional = self.list(&[i, condition, then_block, else_block]);
        self.list(&[a, conditional, all_env])
    }

    fn gen_path(&mut self, scope_id: ScopeId, symbol_id: SymbolId) -> NodePtr {
        let index = self.captures[&scope_id]
            .iter()
            .chain(self.db.scope(scope_id).definitions().iter())
            .position(|&id| id == symbol_id)
            .expect("symbol not found");

        let mut path = 2;
        for _ in 0..index {
            path *= 2;
            path += 1;
        }

        self.allocator
            .new_small_number(path)
            .expect("could not allocate path")
    }

    fn gen_atom(&mut self, value: Vec<u8>) -> NodePtr {
        let int_ptr = self
            .allocator
            .new_atom(&value)
            .expect("could not allocate number");
        self.quote(int_ptr)
    }

    fn quote(&mut self, ptr: NodePtr) -> NodePtr {
        if ptr.is_atom() && self.allocator.atom(ptr).as_ref().is_empty() {
            return ptr;
        }

        let q = self.allocator.one();
        self.allocator
            .new_pair(q, ptr)
            .expect("could not allocate quote")
    }

    fn list(&mut self, items: &[NodePtr]) -> NodePtr {
        let mut ptr = self.allocator.nil();
        for &item in items.iter().rev() {
            ptr = self
                .allocator
                .new_pair(item, ptr)
                .expect("could not allocate pair");
        }
        ptr
    }

    fn runtime_list(&mut self, items: &[NodePtr], end: NodePtr) -> NodePtr {
        let mut ptr = end;
        let c = self
            .allocator
            .new_small_number(4)
            .expect("could not allocate `c`");
        for &item in items.iter().rev() {
            ptr = self.list(&[c, item, ptr]);
        }
        ptr
    }

    fn runtime_runtime_list(&mut self, items: &[NodePtr], end: NodePtr) -> NodePtr {
        let mut ptr = end;
        let c = self
            .allocator
            .new_small_number(4)
            .expect("could not allocate `c`");
        let quoted_c = self.quote(c);
        for &item in items.iter().rev() {
            ptr = self.runtime_list(&[quoted_c, item, ptr], NodePtr::NIL);
        }
        ptr
    }

    fn runtime_quote(&mut self, ptr: NodePtr) -> NodePtr {
        let q = self.allocator.one();
        let c = self
            .allocator
            .new_small_number(4)
            .expect("could not allocate `c`");
        let quoted_q = self.quote(q);
        self.list(&[c, quoted_q, ptr])
    }
}