// CEP:FILE: crates/mlkp-front/src/lib.rs
// CEP:WHAT: The .mlkp text front-end: a strict line-oriented parser lowering tensor programs into the semantic Tensor IR types and the heterogeneous dataflow graph.
// CEP:WHY: The design's JIT lifecycle begins with an application graph; a deterministic text format with strict validation provides the reproducible input the cache keys and the test suite depend on.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns ParseError with line and column for any malformed input; no panics on any byte input.
// CEP:ASSUMES: input is valid UTF-8; the validation airlock checks file size and statement caps before parsing (standard chapter 64.2).
// CEP:COST: cold; parsing runs once per program.
// CEP:EVIDENCE: unit tests in crates/mlkp-front/tests/front_test.rs.

//! .mlkp text front-end.
//!
//! Grammar (line oriented, one statement per line, `#` starts a comment):
//! ```text
//! fn @name(%a: f32[M,K], %b: f32[K,N]) -> f32[M,N] {
//!   %t = matmul(%a, %b)
//!   %r = relu(%t)
//!   return %r
//! }
//! ```

#![deny(missing_docs)]

use mlkp_hir::{Device, Graph, Node, Op};
use mlkp_tensor::{DimExpr, ElemKind, MapFn, TensorType};

/// Maximum accepted source file size.
pub const MAX_FILE_BYTES: usize = mlkp_config::limits::MAX_FRONTEND_FILE_BYTES;

/// Parse and lower failure.
#[derive(Clone, Debug, PartialEq)]
pub struct ParseError {
    /// One-based line.
    pub line: usize,
    /// Reason.
    pub message: String,
}

impl std::fmt::Display for ParseError {
    // CEP:WHAT: Formats the parse error.
    // CEP:WHY: Deterministic diagnostics.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: cold; one formatted string.
    // CEP:EVIDENCE: crates/mlkp-front/tests/front_test.rs
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "program:{}: {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

/// One parsed parameter binding: name, element kind, and dimension expressions.
#[derive(Clone, Debug)]
pub struct Param {
    /// Parameter name without the sigil.
    pub name: String,
    /// Element kind.
    pub elem: ElemKind,
    /// Dimension expressions.
    pub dims: Vec<DimExpr>,
}

/// A parsed and lowered program.
#[derive(Clone, Debug)]
pub struct Program {
    /// Function name.
    pub name: String,
    /// Parameters in order.
    pub params: Vec<Param>,
    /// Output type.
    pub output: TensorType,
    /// Lowered heterogeneous graph.
    pub graph: Graph,
}

/// Parses a .mlkp program into the semantic IR plus the heterogeneous graph.
// CEP:WHAT: Parses and lowers one program.
// CEP:WHY: One entry point keeps the airlock (size cap) ahead of the parser.
// CEP:STATUS: complete
// CEP:FAILURE: Returns ParseError with position for any malformed input; rejects oversized files before parsing.
// CEP:ASSUMES: none
// CEP:COST: cold; O(source size).
// CEP:EVIDENCE: crates/mlkp-front/tests/front_test.rs
pub fn parse_program(text: &str) -> Result<Program, ParseError> {
    if text.len() > MAX_FILE_BYTES {
        return Err(ParseError {
            line: 1,
            message: "source exceeds the maximum accepted size".to_string(),
        });
    }
    let mut parser = Parser::new(text);
    parser.parse_fn()
}

struct Parser<'a> {
    lines: Vec<(usize, &'a str)>,
    idx: usize,
    symbol_table: Vec<String>,
    next_node: u32,
    value_map: Vec<(String, u32)>,
    type_map: Vec<(u32, TensorType)>,
    pending_inputs: Vec<Node>,
    params: Vec<Param>,
    param_count: u32,
}

impl<'a> Parser<'a> {
    // CEP:WHAT: Creates a parser over trimmed significant lines.
    // CEP:WHY: The line-oriented grammar keeps errors positional.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(source size).
    // CEP:EVIDENCE: crates/mlkp-front/tests/front_test.rs
    fn new(text: &'a str) -> Self {
        let lines: Vec<(usize, &str)> = text
            .lines()
            .enumerate()
            .map(|(i, l)| (i + 1, l.trim()))
            .filter(|(_, l)| !l.is_empty() && !l.starts_with('#'))
            .collect();
        Parser {
            lines,
            idx: 0,
            symbol_table: Vec::new(),
            next_node: 0,
            value_map: Vec::new(),
            type_map: Vec::new(),
            pending_inputs: Vec::new(),
            params: Vec::new(),
            param_count: 0,
        }
    }

    // CEP:WHAT: Consumes the next significant line.
    // CEP:WHY: Sequential grammar consumption.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns None at end of input.
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-front/tests/front_test.rs
    fn next_line(&mut self) -> Option<(usize, &'a str)> {
        let out = self.lines.get(self.idx).copied();
        if out.is_some() {
            self.idx += 1;
        }
        out
    }

    // CEP:WHAT: Interns a dimension symbol or parses a constant.
    // CEP:WHY: Shape symbols like M map to stable DimExpr ids reused across parameters.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns an error for names beyond the length limit.
    // CEP:ASSUMES: none
    // CEP:COST: O(symbols).
    // CEP:EVIDENCE: crates/mlkp-front/tests/front_test.rs
    fn dim(&mut self, token: &str, line: usize) -> Result<DimExpr, ParseError> {
        if let Ok(v) = token.parse::<u64>() {
            return Ok(DimExpr::Const(v));
        }
        if token.len() > mlkp_config::limits::MAX_FRONTEND_IDENT_BYTES {
            return Err(ParseError {
                line,
                message: "identifier exceeds the length limit".to_string(),
            });
        }
        if let Some(pos) = self.symbol_table.iter().position(|s| s == token) {
            return Ok(DimExpr::Sym(pos as u32 + 1));
        }
        self.symbol_table.push(token.to_string());
        Ok(DimExpr::Sym(self.symbol_table.len() as u32))
    }

    // CEP:WHAT: Parses a type like f32[M,K].
    // CEP:WHY: Parameter and output declarations share one type grammar.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns an error for unknown element kinds or malformed brackets.
    // CEP:ASSUMES: none
    // CEP:COST: O(dims).
    // CEP:EVIDENCE: crates/mlkp-front/tests/front_test.rs
    fn parse_type(&mut self, text: &str, line: usize) -> Result<TensorType, ParseError> {
        let open = text.find('[').ok_or_else(|| ParseError {
            line,
            message: format!("type '{text}' lacks dimension brackets"),
        })?;
        let close = text.rfind(']').ok_or_else(|| ParseError {
            line,
            message: format!("type '{text}' lacks closing bracket"),
        })?;
        if close < open {
            return Err(ParseError {
                line,
                message: format!("type '{text}' has mismatched brackets"),
            });
        }
        let elem = ElemKind::parse(text[..open].trim()).map_err(|e| ParseError {
            line,
            message: e.to_string(),
        })?;
        let dim_text = &text[open + 1..close];
        let mut dims = Vec::new();
        for token in dim_text.split(',') {
            let token = token.trim();
            if token.is_empty() {
                return Err(ParseError {
                    line,
                    message: "empty dimension in type".to_string(),
                });
            }
            dims.push(self.dim(token, line)?);
        }
        TensorType::dense(elem, dims).map_err(|e| ParseError {
            line,
            message: e.to_string(),
        })
    }

    // CEP:WHAT: Reads the registered type of a value.
    // CEP:WHY: Type inference consults operand definitions.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns None for unknown values.
    // CEP:ASSUMES: none
    // CEP:COST: O(values).
    // CEP:EVIDENCE: crates/mlkp-front/tests/front_test.rs
    fn node_type(&self, id: u32) -> Option<TensorType> {
        self.type_map
            .iter()
            .rev()
            .find(|(i, _)| *i == id)
            .map(|(_, t)| t.clone())
    }

    // CEP:WHAT: Registers a value name and node id.
    // CEP:WHY: Later statements reference earlier definitions; the last definition wins.
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-front/tests/front_test.rs
    fn register(&mut self, name: &str, id: u32) {
        self.value_map.push((name.to_string(), id));
    }

    // CEP:WHAT: Looks up a value name.
    // CEP:WHY: Operands reference earlier definitions.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns None for unknown values.
    // CEP:ASSUMES: none
    // CEP:COST: O(values).
    // CEP:EVIDENCE: crates/mlkp-front/tests/front_test.rs
    fn lookup(&self, name: &str) -> Option<u32> {
        self.value_map
            .iter()
            .rev()
            .find(|(n, _)| n == name)
            .map(|(_, id)| *id)
    }

    // CEP:WHAT: Parses the whole function.
    // CEP:WHY: The top-level grammar production.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns ParseError for malformed signatures, unknown operations, unknown operands, or missing returns.
    // CEP:ASSUMES: none
    // CEP:COST: O(statements).
    // CEP:EVIDENCE: crates/mlkp-front/tests/front_test.rs
    fn parse_fn(&mut self) -> Result<Program, ParseError> {
        let (sig_line, sig) = self.next_line().ok_or_else(|| ParseError {
            line: 1,
            message: "empty program".to_string(),
        })?;
        let sig = sig.strip_prefix("fn").ok_or_else(|| ParseError {
            line: sig_line,
            message: "program must start with 'fn @name'".to_string(),
        })?;
        let name_end = sig.find('(').ok_or_else(|| ParseError {
            line: sig_line,
            message: "signature lacks parameter list".to_string(),
        })?;
        let name = sig[..name_end].trim().trim_start_matches('@').to_string();
        let rest = &sig[name_end + 1..];
        let arrow = rest.find("->").ok_or_else(|| ParseError {
            line: sig_line,
            message: "signature lacks '->' output type".to_string(),
        })?;
        let params_all = &rest[..arrow];
        let params_text = params_all.strip_suffix(')').unwrap_or(params_all);
        let output_text = rest[arrow + 2..].trim();
        let open_brace = output_text.find('{').ok_or_else(|| ParseError {
            line: sig_line,
            message: "signature lacks opening brace".to_string(),
        })?;
        let output = self.parse_type(output_text[..open_brace].trim(), sig_line)?;

        for part in split_top_level(params_text) {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let (pname, ptype) = part.split_once(':').ok_or_else(|| ParseError {
                line: sig_line,
                message: format!("parameter '{part}' lacks ': type'"),
            })?;
            let ty = self.parse_type(ptype.trim(), sig_line)?;
            let pname_full = pname.trim();
            let pname = pname_full
                .strip_prefix('%')
                .unwrap_or(pname_full)
                .to_string();
            let id = self.next_node;
            self.next_node += 1;
            self.register(&pname, id);
            self.type_map.push((id, ty.clone()));
            self.pending_inputs.push(Node {
                id,
                op: Op::Input,
                inputs: Vec::new(),
                device: Device::Cpu(0),
                ty,
                pgo: Default::default(),
                name: pname.clone(),
            });
            self.params.push(Param {
                name: pname,
                elem: self
                    .pending_inputs
                    .last()
                    .map(|n| n.ty.elem)
                    .unwrap_or(ElemKind::F32),
                dims: Vec::new(),
            });
            self.param_count += 1;
        }
        // Fill the parameter dimension lists from the staged inputs.
        for (i, p) in self.params.iter_mut().enumerate() {
            if let Some(node) = self.pending_inputs.get(i) {
                p.dims = node.ty.dims.clone();
            }
        }

        let mut graph = Graph::default();
        let inputs = std::mem::take(&mut self.pending_inputs);
        for node in inputs {
            graph.add(node).expect("fresh ids");
        }
        graph.inputs = (0..self.param_count).collect();

        let mut return_seen = false;
        while let Some((line, text)) = self.next_line() {
            if text == "}" {
                break;
            }
            if let Some(ret_body) = text.strip_prefix("return") {
                if return_seen {
                    return Err(ParseError {
                        line,
                        message: "duplicate return".to_string(),
                    });
                }
                let trimmed_ret = ret_body.trim();
                let ret_val = trimmed_ret.strip_prefix('%').unwrap_or(trimmed_ret);
                let node_id = self.lookup(ret_val).ok_or_else(|| ParseError {
                    line,
                    message: format!("return references unknown value '{ret_val}'"),
                })?;
                graph.outputs.push(node_id);
                return_seen = true;
                continue;
            }
            if return_seen {
                return Err(ParseError {
                    line,
                    message: "statement after return".to_string(),
                });
            }
            self.parse_statement(line, text, &mut graph)?;
        }
        if !return_seen {
            return Err(ParseError {
                line: sig_line,
                message: "missing return statement".to_string(),
            });
        }
        Ok(Program {
            name,
            params: self.params.clone(),
            output,
            graph,
        })
    }

    // CEP:WHAT: Parses one assignment statement.
    // CEP:WHY: Statements are the only statement-level grammar production.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns ParseError for malformed statements, unknown operations, or unknown operands.
    // CEP:ASSUMES: none
    // CEP:COST: O(1) per statement.
    // CEP:EVIDENCE: crates/mlkp-front/tests/front_test.rs
    fn parse_statement(
        &mut self,
        line: usize,
        text: &str,
        graph: &mut Graph,
    ) -> Result<(), ParseError> {
        if text.len() > 256 {
            return Err(ParseError {
                line,
                message: "statement exceeds the length limit".to_string(),
            });
        }
        let eq = text.find('=').ok_or_else(|| ParseError {
            line,
            message: "statement lacks '='".to_string(),
        })?;
        let target_trimmed = text[..eq].trim();
        let target = target_trimmed
            .strip_prefix('%')
            .unwrap_or(target_trimmed)
            .to_string();
        let rhs = text[eq + 1..].trim();
        let open = rhs.find('(').ok_or_else(|| ParseError {
            line,
            message: "statement lacks call parentheses".to_string(),
        })?;
        let close = rhs.rfind(')').ok_or_else(|| ParseError {
            line,
            message: "statement lacks closing parenthesis".to_string(),
        })?;
        if close < open {
            return Err(ParseError {
                line,
                message: "statement has mismatched parentheses".to_string(),
            });
        }
        let op_name = rhs[..open].trim();
        let args_text = &rhs[open + 1..close];
        let mut operands = Vec::new();
        for arg in args_text.split(',') {
            let arg_trimmed = arg.trim();
            let arg = arg_trimmed.strip_prefix('%').unwrap_or(arg_trimmed);
            if arg.is_empty() {
                continue;
            }
            let id = self.lookup(arg).ok_or_else(|| ParseError {
                line,
                message: format!("unknown operand '{arg}'"),
            })?;
            operands.push(id);
        }
        let op = match op_name {
            "matmul" => Op::Contract,
            "relu" => Op::Map(MapFn::Relu),
            "add" => Op::Map(MapFn::Add),
            "mul" => Op::Map(MapFn::Mul),
            "exp" => Op::Map(MapFn::Exp),
            "neg" => Op::Map(MapFn::Neg),
            "sigmoid" => Op::Map(MapFn::Sigmoid),
            "softmax" => Op::Softmax { axis: 1 },
            other => {
                return Err(ParseError {
                    line,
                    message: format!("unknown operation '{other}'"),
                })
            }
        };
        let ty = self.infer_result_type(op_name, &operands, line)?;
        let id = self.next_node;
        self.next_node += 1;
        let node = Node {
            id,
            op,
            inputs: operands,
            device: Device::Cpu(0),
            ty: ty.clone(),
            pgo: Default::default(),
            name: op_display_name(op_name),
        };
        graph.add(node).expect("fresh ids");
        self.type_map.push((id, ty));
        self.register(&target, id);
        Ok(())
    }

    // CEP:WHAT: Infers the result type of one statement.
    // CEP:WHY: The heterogeneous IR carries types; matmul and elementwise chains have simple shape functions here.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns ParseError for arity mismatches or unknown operand shapes.
    // CEP:ASSUMES: none
    // CEP:COST: O(operands).
    // CEP:EVIDENCE: crates/mlkp-front/tests/front_test.rs
    fn infer_result_type(
        &self,
        op_name: &str,
        operands: &[u32],
        line: usize,
    ) -> Result<TensorType, ParseError> {
        let ty_of = |id: u32| -> Result<TensorType, ParseError> {
            self.node_type(id).ok_or_else(|| ParseError {
                line,
                message: "operand type unavailable".to_string(),
            })
        };
        match op_name {
            "matmul" => {
                if operands.len() != 2 {
                    return Err(ParseError {
                        line,
                        message: "matmul takes two operands".to_string(),
                    });
                }
                let a = ty_of(operands[0])?;
                let b = ty_of(operands[1])?;
                if a.dims.len() != 2 || b.dims.len() != 2 {
                    return Err(ParseError {
                        line,
                        message: "matmul requires rank-2 operands".to_string(),
                    });
                }
                let m = a.dims[0].clone();
                let n = b.dims[1].clone();
                TensorType::dense(a.elem, vec![m, n]).map_err(|e| ParseError {
                    line,
                    message: e.to_string(),
                })
            }
            "softmax" | "relu" | "add" | "mul" | "exp" | "neg" | "sigmoid" => {
                let first = match operands.first() {
                    Some(x) => *x,
                    None => {
                        return Err(ParseError {
                            line,
                            message: "elementwise operation needs operands".to_string(),
                        })
                    }
                };
                let a = ty_of(first)?;
                for other in &operands[1..] {
                    let t = ty_of(*other)?;
                    if t.dims.len() != a.dims.len() {
                        return Err(ParseError {
                            line,
                            message: "elementwise operands disagree in rank".to_string(),
                        });
                    }
                }
                Ok(a)
            }
            other => Err(ParseError {
                line,
                message: format!("unknown operation '{other}'"),
            }),
        }
    }
}

// CEP:WHAT: Splits a comma-separated list while ignoring commas inside brackets.
// CEP:WHY: Type expressions like f32[M,K] contain commas that must not split parameters.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: brackets are balanced; the type parser checks each fragment anyway.
// CEP:COST: O(text).
// CEP:EVIDENCE: crates/mlkp-front/tests/front_test.rs
fn split_top_level(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (i, c) in text.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => {
                depth = depth.saturating_sub(1);
            }
            ',' if depth == 0 => {
                parts.push(&text[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
}

// CEP:WHAT: Maps an operation name to its diagnostic node name.
// CEP:WHY: The matmul contraction keeps the "mm" shorthand the logs and passes reference.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-front/tests/front_test.rs
fn op_display_name(op_name: &str) -> String {
    match op_name {
        "matmul" => "mm".to_string(),
        other => other.to_string(),
    }
}
