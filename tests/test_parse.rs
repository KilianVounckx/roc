#![feature(box_syntax, box_patterns)]

#[macro_use] extern crate pretty_assertions;
#[macro_use] extern crate combine;

extern crate roc;

#[cfg(test)]
mod tests {
    #![feature(box_syntax, box_patterns)]

    use roc::expr::Expr::*;
    use roc::expr::Operator::*;
    use roc::parse;
    use combine::{Parser};

    #[test]
    fn test_parse_positive_int() {
        assert_eq!(Ok((Int(1234), "")), parse::number_literal().parse("1234"));
    }

    #[test]
    fn test_parse_negative_int() {
        assert_eq!(Ok((Int(-1234), "")), parse::number_literal().parse("-1234"));
    }

    #[test]
    fn test_parse_single_operator() {
        match parse::expr().parse("1234 + 567") {
            Ok((CallOperator(v1, op, v2), "")) => {
                assert_eq!(*v1, Int(1234));
                assert_eq!(op, Plus);
                assert_eq!(*v2, Int(567));
            },
            _ => panic!("Expression didn't parse"),
        }
    }

    #[test]
    fn test_parse_multiple_operators() {
        #![feature(box_syntax, box_patterns)]
        match parse::expr().parse("1 + 2 * 3") {
            Ok((CallOperator(box v1, op1, box CallOperator(box v2, op2, box v3)), "")) => {
                assert_eq!(v1, Int(1));
                assert_eq!(op1, Plus);
                assert_eq!(v2, Int(2));
                assert_eq!(op2, Star);
                assert_eq!(v3, Int(3));
            },
            _ => panic!("Expression didn't parse"),
        }
    }
}
