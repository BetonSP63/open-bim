//! Formulas of custom components and families: numbers, named values,
//! `+ - * / ^`, brackets, the comparisons `< > = <> <= >=` (1 when true,
//! otherwise 0) and the functions `min`, `max`, `abs`, `sqrt`, `round`,
//! `ceil`, `floor`, `if(c, a, b)` (a when c > 0, otherwise b), `and`, `or`,
//! `not` and `sin`, `cos`, `tan`, `atan` in degrees. Arguments are
//! separated by `,` or `;`. Names are letters (Latin or Cyrillic), digits
//! and `_`, starting with a letter or `_`.
use std::collections::BTreeMap;

/// Longest formula accepted, characters.
pub const MAX_FORMULA: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Token {
    Number(f64),
    Name(usize, usize),
    Op(char),
}

fn tokens(src: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() || c == '.' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            out.push(Token::Number(
                text.parse()
                    .map_err(|_| format!("Неверное число «{text}»"))?,
            ));
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            out.push(Token::Name(start, i));
        } else if "+-*/^(),;".contains(c) {
            out.push(Token::Op(if c == ';' { ',' } else { c }));
            i += 1;
        } else if "<>=".contains(c) {
            // Two-character comparisons become one sign.
            let next = chars.get(i + 1).copied();
            let (op, len) = match (c, next) {
                ('<', Some('=')) => ('≤', 2),
                ('>', Some('=')) => ('≥', 2),
                ('<', Some('>')) => ('≠', 2),
                _ => (c, 1),
            };
            out.push(Token::Op(op));
            i += len;
        } else {
            return Err(format!("Недопустимый знак «{c}»"));
        }
    }
    Ok(out)
}

struct Parser<'a> {
    chars: Vec<char>,
    tokens: Vec<Token>,
    at: usize,
    values: &'a dyn Fn(&str) -> Option<f64>,
    depth: usize,
    /// Inside the branch `if` does not take: arithmetic errors there give 0,
    /// so `if(n > 1, B / (n - 1), 0)` works at n = 1.
    muted: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<Token> {
        self.tokens.get(self.at).copied()
    }
    fn eat(&mut self, op: char) -> bool {
        if self.peek() == Some(Token::Op(op)) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    /// `if(c, a, b)` after its «(»: only the branch taken may fail.
    fn branch(&mut self) -> Result<f64, String> {
        let wrong = || Err("«if» принимает три значения: условие, да, нет".into());
        let c = self.compare()?;
        let mut picked = 0.;
        for take in [c > 0., c <= 0.] {
            if !self.eat(',') {
                return wrong();
            }
            self.muted += usize::from(!take);
            let v = self.compare();
            self.muted -= usize::from(!take);
            if take {
                picked = v?;
            } else {
                v?;
            }
        }
        if !self.eat(')') {
            return wrong();
        }
        Ok(picked)
    }
    /// A sum, or two sums compared.
    fn compare(&mut self) -> Result<f64, String> {
        let a = self.expr()?;
        let Some(Token::Op(op @ ('<' | '>' | '=' | '≤' | '≥' | '≠'))) = self.peek() else {
            return Ok(a);
        };
        self.at += 1;
        let b = self.expr()?;
        // Lengths come from formulas, so equal means equal to a micron.
        let eq = (a - b).abs() < 1e-6;
        let yes = match op {
            '<' => a < b && !eq,
            '>' => a > b && !eq,
            '=' => eq,
            '≤' => a < b || eq,
            '≥' => a > b || eq,
            _ => !eq,
        };
        Ok(if yes { 1. } else { 0. })
    }
    fn expr(&mut self) -> Result<f64, String> {
        self.depth += 1;
        if self.depth > 64 {
            return Err("Слишком глубокая вложенность".into());
        }
        let mut v = self.term()?;
        loop {
            if self.eat('+') {
                v += self.term()?;
            } else if self.eat('-') {
                v -= self.term()?;
            } else {
                break;
            }
        }
        self.depth -= 1;
        Ok(v)
    }
    fn term(&mut self) -> Result<f64, String> {
        let mut v = self.unary()?;
        loop {
            if self.eat('*') {
                v *= self.unary()?;
            } else if self.eat('/') {
                let d = self.unary()?;
                if d == 0. {
                    if self.muted == 0 {
                        return Err("Деление на ноль".into());
                    }
                    v = 0.;
                } else {
                    v /= d;
                }
            } else {
                break;
            }
        }
        Ok(v)
    }
    fn unary(&mut self) -> Result<f64, String> {
        if self.eat('-') {
            return Ok(-self.unary()?);
        }
        if self.eat('+') {
            return self.unary();
        }
        let base = self.atom()?;
        if self.eat('^') {
            return Ok(base.powf(self.unary()?));
        }
        Ok(base)
    }
    fn atom(&mut self) -> Result<f64, String> {
        match self.peek() {
            Some(Token::Number(v)) => {
                self.at += 1;
                Ok(v)
            }
            Some(Token::Op('(')) => {
                self.at += 1;
                let v = self.compare()?;
                if !self.eat(')') {
                    return Err("Не закрыта скобка".into());
                }
                Ok(v)
            }
            Some(Token::Name(a, b)) => {
                self.at += 1;
                let name: String = self.chars[a..b].iter().collect();
                if name == "if" && self.eat('(') {
                    return self.branch();
                }
                if self.eat('(') {
                    let mut args = Vec::new();
                    if !self.eat(')') {
                        loop {
                            args.push(self.compare()?);
                            if self.eat(')') {
                                break;
                            }
                            if !self.eat(',') {
                                return Err(format!("В «{name}(…)» ожидается «,» или «)»"));
                            }
                        }
                    }
                    match call(&name, &args) {
                        Err(_) if self.muted > 0 => Ok(0.),
                        result => result,
                    }
                } else {
                    (self.values)(&name).ok_or_else(|| format!("Неизвестное имя «{name}»"))
                }
            }
            Some(Token::Op(c)) => {
                let sign = match c {
                    '≤' => "<=".to_string(),
                    '≥' => ">=".to_string(),
                    '≠' => "<>".to_string(),
                    _ => c.to_string(),
                };
                Err(format!("Неожиданный знак «{sign}»"))
            }
            None => Err("Формула оборвана".into()),
        }
    }
}

fn call(name: &str, args: &[f64]) -> Result<f64, String> {
    let one = || {
        if args.len() == 1 {
            Ok(args[0])
        } else {
            Err(format!("«{name}» принимает одно значение"))
        }
    };
    match name {
        "min" | "max" if args.is_empty() => Err(format!("«{name}» без значений")),
        "min" => Ok(args.iter().copied().fold(f64::INFINITY, f64::min)),
        "max" => Ok(args.iter().copied().fold(f64::NEG_INFINITY, f64::max)),
        "abs" => one().map(f64::abs),
        "sqrt" => one().and_then(|v| {
            if v < 0. {
                Err("Корень из отрицательного числа".into())
            } else {
                Ok(v.sqrt())
            }
        }),
        "round" => one().map(f64::round),
        "ceil" => one().map(f64::ceil),
        "floor" => one().map(f64::floor),
        "and" | "or" if args.is_empty() => Err(format!("«{name}» без значений")),
        "and" => Ok(f64::from(u8::from(args.iter().all(|v| *v > 0.)))),
        "or" => Ok(f64::from(u8::from(args.iter().any(|v| *v > 0.)))),
        "not" => one().map(|v| f64::from(u8::from(v <= 0.))),
        "sin" => one().map(|v| v.to_radians().sin()),
        "cos" => one().map(|v| v.to_radians().cos()),
        "tan" => one().and_then(|v| {
            if (v.rem_euclid(180.) - 90.).abs() < 1e-9 {
                Err("Тангенс 90° не определён".into())
            } else {
                Ok(v.to_radians().tan())
            }
        }),
        "atan" => one().map(|v| v.atan().to_degrees()),
        _ => Err(format!("Неизвестная функция «{name}»")),
    }
}

/// Value of `src` with names looked up in `values`.
pub fn eval_with(src: &str, values: &dyn Fn(&str) -> Option<f64>) -> Result<f64, String> {
    if src.chars().count() > MAX_FORMULA {
        return Err(format!("Формула длиннее {MAX_FORMULA} знаков"));
    }
    let mut p = Parser {
        chars: src.chars().collect(),
        tokens: tokens(src)?,
        at: 0,
        values,
        depth: 0,
        muted: 0,
    };
    if p.tokens.is_empty() {
        return Err("Пустая формула".into());
    }
    let v = p.compare()?;
    if p.at != p.tokens.len() {
        return Err("Лишнее в конце формулы".into());
    }
    if !v.is_finite() {
        return Err("Значение не число".into());
    }
    Ok(v)
}

/// Value of `src` with names from `values`.
pub fn eval(src: &str, values: &BTreeMap<String, f64>) -> Result<f64, String> {
    eval_with(src, &|name| values.get(name).copied())
}

const FUNCTIONS: [&str; 15] = [
    "min", "max", "abs", "sqrt", "round", "ceil", "floor", "if", "and", "or", "not", "sin", "cos",
    "tan", "atan",
];

/// Whether `name` can be used as a variable name in formulas.
pub fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || c == '_')
        && name.chars().count() <= 32
        && !FUNCTIONS.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(src: &str) -> Result<f64, String> {
        let values = BTreeMap::from([("h".to_string(), 300.), ("ширина".to_string(), 150.)]);
        eval(src, &values)
    }

    #[test]
    fn arithmetic_names_and_functions() {
        assert_eq!(v("1 + 2 * 3").unwrap(), 7.);
        assert_eq!(v("(1 + 2) * 3").unwrap(), 9.);
        assert_eq!(v("-h / 2").unwrap(), -150.);
        assert_eq!(v("2 ^ 3 ^ 2").unwrap(), 512.);
        assert_eq!(v("max(h; ширина, 10) - min(1, 2)").unwrap(), 299.);
        assert_eq!(v("ceil(h / 7)").unwrap(), 43.);
        assert_eq!(v("if(h - 400, 1, 2)").unwrap(), 2.);
        assert_eq!(
            v("sqrt(9) + abs(-1) + round(2.5) + floor(1.9)").unwrap(),
            8.
        );
    }

    #[test]
    fn comparisons_logic_and_degrees() {
        assert_eq!(v("h > 200").unwrap(), 1.);
        assert_eq!(v("h <= 200").unwrap(), 0.);
        assert_eq!(v("h = 300").unwrap(), 1.);
        assert_eq!(v("h <> 300").unwrap(), 0.);
        assert_eq!(v("if(h >= 300; 1; 2)").unwrap(), 1.);
        assert_eq!(v("and(h > 0, ширина < 100)").unwrap(), 0.);
        assert_eq!(v("or(h > 0, ширина < 100)").unwrap(), 1.);
        assert_eq!(v("not(h < 1)").unwrap(), 1.);
        assert!((v("sin(30)").unwrap() - 0.5).abs() < 1e-12);
        assert!((v("atan(1)").unwrap() - 45.).abs() < 1e-12);
        assert!(v("tan(90)").is_err());
        assert_eq!(v("if(h > 1000, h / 0, 5)").unwrap(), 5.);
        assert_eq!(v("if(h > 1000, sqrt(-1), 5)").unwrap(), 5.);
        assert!(v("if(h > 0, h / 0, 5)").is_err());
        assert!(v("if(h > 1000, x, 5)").is_err());
        assert!(v("if(1, 2)").is_err());
        assert!(v("h > > 1").unwrap_err().contains("«>»"));
        assert!(v("<= 1").unwrap_err().contains("«<=»"));
        assert!(!valid_name("not") && !valid_name("sin"));
    }

    #[test]
    fn errors_name_the_problem() {
        assert!(v("h +").unwrap_err().contains("оборвана"));
        assert!(v("(h").unwrap_err().contains("скобка"));
        assert!(v("x * 2").unwrap_err().contains("«x»"));
        assert!(v("h / 0").unwrap_err().contains("ноль"));
        assert!(v("foo(1)").unwrap_err().contains("функция"));
        assert!(v("h h").unwrap_err().contains("Лишнее"));
        assert!(v("1 # 2").unwrap_err().contains("«#»"));
        assert!(v("").is_err());
        assert!(valid_name("отступ_2") && !valid_name("2a") && !valid_name("max"));
    }
}
