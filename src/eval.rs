use expr;
use expr::{Expr, Operator, Pattern, Problem};
use expr::Expr::*;
use expr::Problem::*;
use expr::Pattern::*;
use expr::Operator::*;
use std::rc::Rc;
use im_rc::hashmap::HashMap;
use self::Evaluated::*;
use smallvec::SmallVec;

pub fn eval(expr: Expr) -> Evaluated {
    scoped_eval(expr, &HashMap::new())
}

#[inline(always)]
pub fn from_evaluated(Evaluated(expr): Evaluated) -> Expr {
    expr
}

/// Wrapper indicating the expression has been evaluated
pub enum Evaluated { Evaluated(Expr) }

type Scope = HashMap<String, Rc<Evaluated>>;

#[inline(always)]
fn problem(prob: Problem) -> Evaluated {
    Evaluated(Error(prob))
}

pub fn scoped_eval(expr: Expr, vars: &Scope) -> Evaluated {
    match expr {
        // Primitives need no further evaluation
        Error(_) | Int(_) | EmptyStr | Str(_) | Frac(_, _) | Char(_) | Closure(_, _) | Expr::EmptyRecord => Evaluated(expr),

        // Resolve variable names
        Var(name) => match vars.get(&name) {
            Some(resolved) => {
                let Evaluated(ref evaluated_expr) = **resolved;

                // TODO is there any way to  avoid this clone? (Do we care,
                // once this is a canonical expression with Rc instead of Box?
                Evaluated(evaluated_expr.clone())
            },
            None => problem(UnrecognizedVarName(name))
        },

        InterpolatedStr(pairs, trailing_str) => {
            let mut output = String::new();

            for (string, var_name) in pairs.into_iter() {
                match vars.get(&var_name) {
                    Some(resolved) => {
                        match **resolved {
                            Evaluated(Str(ref var_string)) => {
                                output.push_str(string.as_str());
                                output.push_str(var_string.as_str());
                            },
                            _ => {
                                return problem(TypeMismatch(var_name));
                            }
                        }
                    },
                    None => { return problem(UnrecognizedVarName(var_name)); }
                }
            }

            output.push_str(trailing_str.as_str());

            Evaluated(Str(output))
        },

        Assign(Identifier(name), definition, in_expr) => {
            if vars.contains_key(&name) {
                problem(ReassignedVarName(name))
            } else {
                // Create a new scope containing the new declaration.
                let mut new_vars = vars.clone();
                let evaluated_defn = scoped_eval(*definition, vars);

                new_vars.insert(name, Rc::new(evaluated_defn));

                // Evaluate in_expr with that new scope's variables.
                scoped_eval(*in_expr, &new_vars)
            }
        },

        Assign(Integer(_), _, _) => {
            panic!("You cannot assign integers to other values!");
        },

        Assign(Fraction(_, _), _, _) => {
            panic!("You cannot assign fractions to other values!");
        },

        Assign(Variant(_name, _patterns), _definition, _in_expr) => {
            panic!("Pattern matching on variants is not yet supported!");
        },

        Assign(Underscore, definition, in_expr) => {
            // Faithfully eval this, but discard its result.
            scoped_eval(*definition, &vars);

            // Actually use this part.
            scoped_eval(*in_expr, vars)
        },

        Assign(Pattern::EmptyRecord, definition, in_expr) => {
            // Faithfully eval this, but discard its result.
            scoped_eval(*definition, &vars);

            // Actually use this part.
            scoped_eval(*in_expr, vars)
        },

        CallByName(name, args) => {
            let func_expr = match vars.get(&name) {
                Some(resolved) => {
                    let Evaluated(ref evaluated_expr) = **resolved;

                    // TODO is there any way to  avoid this clone? (Do we care,
                    // once this is a canonical expression with Rc instead of Box?
                    Evaluated(evaluated_expr.clone())
                },
                None => problem(UnrecognizedVarName(name))
            };

            eval_apply(func_expr, args, vars)
        },

        ApplyVariant(_, None) => Evaluated(expr), // This is all we do - for now...
        ApplyVariant(name, Some(exprs)) => {
            Evaluated(
                ApplyVariant(
                    name,
                    Some(
                        exprs.into_iter().map(|arg| {
                            let Evaluated(final_expr) = scoped_eval(arg, vars);

                            final_expr
                        }).collect()
                    )
                )
            )
        }

        Apply(func_expr, args) => {
            eval_apply(scoped_eval(*func_expr, vars), args, vars)
        },

        Case(condition, branches) => {
            eval_case(scoped_eval(*condition, vars), branches, vars)
        },

        Operator(left_arg, op, right_arg) => {
            eval_operator(
                &scoped_eval(*left_arg, vars),
                op,
                &scoped_eval(*right_arg, vars)
            )
        },

        If(condition, if_true, if_false) => {
            match scoped_eval(*condition, vars) {
                Evaluated(ApplyVariant(variant_name, None)) => {
                    match variant_name.as_str() {
                        "True" => scoped_eval(*if_true, vars),
                        "False" => scoped_eval(*if_false, vars),
                        _ => problem(TypeMismatch("non-Bool used in `if` condition".to_string()))
                    }
                },
                _ => problem(TypeMismatch("non-Bool used in `if` condition".to_string()))
            }
        }
    }
}

#[inline(always)]
fn eval_apply(evaluated: Evaluated, args: Vec<Expr>, vars: &Scope) -> Evaluated {
    match evaluated {
        Evaluated(Closure(arg_patterns, body)) => {
            let evaluated_args =
                args.into_iter()
                    .map(|arg| scoped_eval(arg, vars))
                    .collect();

            match eval_closure(evaluated_args, arg_patterns, vars) {
                Ok(new_vars) => scoped_eval(*body, &new_vars),
                Err(prob) => problem(prob)
            }
        },
        Evaluated(expr) => {
            problem(TypeMismatch(format!("Tried to call a non-function: {}", expr)))
        }
    }
}

#[inline(always)]
fn eval_closure(args: Vec<Evaluated>, arg_patterns: SmallVec<[Pattern; 2]>, vars: &Scope)
    -> Result<Scope, expr::Problem>
{
    if arg_patterns.len() == args.len() {
        // Create a new scope for the function to use.
        let mut new_vars = vars.clone();

        for ( arg, pattern ) in args.into_iter().zip(arg_patterns) {
            pattern_match(arg, pattern, &mut new_vars)?;
        }

        Ok(new_vars)
    } else {
        Err(WrongArity(arg_patterns.len() as u32, args.len() as u32))
    }
}

fn bool_variant(is_true: bool) -> Expr {
    if is_true {
        ApplyVariant("True".to_string(), None)
    } else {
        ApplyVariant("False".to_string(), None)
    }
}

fn eq(expr1: &Expr, expr2: &Expr) -> Expr {
    match (expr1, expr2) {
        // All functions are defined as equal
        (Closure(_, _), Closure(_, _)) => bool_variant(true),


        (ApplyVariant(left, None), ApplyVariant(right, None)) => {
            bool_variant(left == right)
        },

        (ApplyVariant(left, Some(left_args)), ApplyVariant(right, Some(right_args))) => {
            bool_variant(left == right && left_args.len() == right_args.len())
        },

        (ApplyVariant(_, None), ApplyVariant(_, Some(_))) => {
            bool_variant(false)
        },

        (ApplyVariant(_, Some(_)), ApplyVariant(_, None)) => {
            bool_variant(false)
        },

        (Int(left), Int(right)) => bool_variant(left == right),
        (Str(left), Str(right)) => bool_variant(left == right),
        (Char(left), Char(right)) => bool_variant(left == right),
        (Frac(_, _), Frac(_, _)) => panic!("Don't know how to == on Fracs yet"),

        (_, _) => Error(TypeMismatch("tried to use == on two values with incompatible types".to_string())),
    }
}

#[inline(always)]
fn eval_operator(Evaluated(left_expr): &Evaluated, op: Operator, Evaluated(right_expr): &Evaluated) -> Evaluated {
    // TODO in the future, replace these with named function calls to stdlib
    match (left_expr, op, right_expr) {
        // Error
        (Error(prob), _, _) => problem(prob.clone()),
        (_, _, Error(prob)) => problem(prob.clone()),

        // Equals
        (_, Equals, _) => Evaluated(eq(left_expr, right_expr)),

        // Plus
        (Int(left_num), Plus, Int(right_num)) => Evaluated(Int(left_num + right_num)),
        (Frac(_, _), Plus, Frac(_, _)) => panic!("Don't know how to add fracs yet"),

        (Int(_), Plus, Frac(_, _)) => panic!("Tried to add Int and Frac"),

        (Frac(_, _), Plus, Int(_)) => panic!("Tried to add Frac and Int"),

        (_, Plus, _) => panic!("Tried to add non-numbers"),

        // Star
        (Int(left_num), Star, Int(right_num)) => Evaluated(Int(left_num * right_num)),
        (Frac(_, _), Star, Frac(_, _)) => panic!("Don't know how to multiply fracs yet"),

        (Int(_), Star, Frac(_, _)) => panic!("Tried to multiply Int and Frac"),

        (Frac(_, _), Star, Int(_)) => panic!("Tried to multiply Frac and Int"),

        (_, Star, _) => panic!("Tried to multiply non-numbers"),

        // Minus
        (Int(left_num), Minus, Int(right_num)) => Evaluated(Int(left_num - right_num)),
        (Frac(_, _), Minus, Frac(_, _)) => panic!("Don't know how to subtract fracs yet"),

        (Int(_), Minus, Frac(_, _)) => panic!("Tried to subtract Frac from Int"),

        (Frac(_, _), Minus, Int(_)) => panic!("Tried to subtract Int from Frac"),

        (_, Minus, _) => panic!("Tried to subtract non-numbers"),

        // Slash
        (Int(left_num), Slash, Int(right_num)) => Evaluated(Int(left_num / right_num)),
        (Frac(_, _), Slash, Frac(_, _)) => panic!("Don't know how to divide fracs yet"),

        (Int(_), Slash, Frac(_, _)) => panic!("Tried to divide Int by Frac"),

        (Frac(_, _), Slash, Int(_)) => panic!("Tried to divide Frac by Int"),

        (_, Slash, _) => panic!("Tried to divide non-numbers"),

        // DoubleSlash
        (Int(left_num), DoubleSlash, Int(right_num)) => Evaluated(Int(left_num / right_num)),
        (Frac(_, _), DoubleSlash, Frac(_, _)) => panic!("Tried to do integer division on fracs"),

        (Int(_), DoubleSlash, Frac(_, _)) => panic!("Tried to integer-divide Int by Frac"),

        (Frac(_, _), DoubleSlash, Int(_)) => panic!("Tried to integer-divide Frac by Int"),

        (_, DoubleSlash, _) => panic!("Tried to integer-divide non-numbers"),
    }
}

#[inline(always)]
fn eval_case (condition: Evaluated, branches: SmallVec<[(Pattern, Box<Expr>); 2]>, vars: &Scope) -> Evaluated {
    let Evaluated(ref evaluated_expr) = condition;

    for (pattern, definition) in branches {
        let mut branch_vars = vars.clone();

        // TODO is there any way to  avoid this clone? (Do we care,
        // once this is a canonical expression with Rc instead of Box?
        let cloned_expr = Evaluated(evaluated_expr.clone());

        if pattern_match(cloned_expr, pattern, &mut branch_vars).is_ok() {
            return scoped_eval(*definition, &branch_vars);
        }
    }

    problem(NoBranchesMatched)
}

fn pattern_match(evaluated: Evaluated, pattern: Pattern, vars: &mut Scope) -> Result<(), expr::Problem> {
    match pattern {
        Identifier(name) => {
            vars.insert(name, Rc::new(evaluated));

            Ok(())
        },
        Underscore => {
            // Underscore matches anything, and records no new vars.
            Ok(())
        },
        Pattern::EmptyRecord => {
            match evaluated {
                Evaluated(Expr::EmptyRecord) => Ok(()),
                Evaluated(expr) => Err(TypeMismatch(
                    format!("Wanted a `{}`, but was given `{}`.", "{}", expr)
                ))
            }
        },

        Integer(pattern_num) => {
            match evaluated {
                Evaluated(Expr::Int(evaluated_num)) => {
                    if pattern_num == evaluated_num {
                        Ok(())
                    } else {
                        Err(Problem::NotEqual)
                    }
                },

                Evaluated(expr) => Err(TypeMismatch(
                    format!("Wanted a `{}`, but was given `{}`.", "{}", expr)
                ))
            }
        },

        Fraction(_pattern_numerator, _pattern_denominator) => {
            match evaluated {
                Evaluated(Expr::Frac(_evaluated_numerator, _evaluated_denominator)) => {
                    panic!("Can't handle pattern matching on fracs yet.");
                },

                Evaluated(expr) => Err(TypeMismatch(
                    format!("Wanted a `{}`, but was given `{}`.", "{}", expr)
                ))
            }
        }

        Variant(pattern_variant_name, opt_pattern_contents) => {
            match evaluated {
                Evaluated(ApplyVariant(applied_variant_name, opt_applied_contents)) => {
                    if *pattern_variant_name != applied_variant_name {
                        return Err(TypeMismatch(
                                format!("Wanted a `{}` variant, but was given a `{}` variant.",
                                        pattern_variant_name,
                                        applied_variant_name
                                )
                            )
                        );
                    }

                    match (opt_pattern_contents, opt_applied_contents) {
                        ( Some(pattern_contents), Some(applied_contents) ) => {
                            if pattern_contents.len() == applied_contents.len() {
                                // Recursively pattern match
                                for ( pattern_val, applied_val ) in pattern_contents.into_iter().zip(applied_contents) {
                                    let evaluated_applied_val =
                                        // We know this was already evaluated
                                        // because we are matching on Evaluated(ApplyVariant)
                                        Evaluated(applied_val);

                                    pattern_match(evaluated_applied_val, pattern_val, vars)?;
                                }

                                Ok(())
                            } else {
                                Err(WrongArity(
                                        pattern_contents.len() as u32,
                                        applied_contents.len() as u32
                                    )
                                )
                            }
                        },
                        ( None, None ) => {
                            // It's the variant we expected, but it has no values in it,
                            // so we don't insert anything into vars.
                            Ok(())
                        },
                        ( None, Some(contents) ) => {
                            // It's the variant we expected, but the arity is wrong.
                            Err(WrongArity(contents.len() as u32, 0))
                        },
                        ( Some(patterns), None ) => {
                            // It's the variant we expected, but the arity is wrong.
                            Err(WrongArity(0, patterns.len() as u32))
                        },
                    }
                },
                _ => {
                    Err(TypeMismatch(format!("Wanted to destructure a `{}` variant, but was given a non-variant.", pattern_variant_name)))
                }
            }
        }
    }
}

