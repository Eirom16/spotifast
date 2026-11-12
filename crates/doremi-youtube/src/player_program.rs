//! Adapt the current shared URL-transform interface to the EJS result contract.
//! Only Google's public function definitions run; browser startup calls are
//! excluded. Older players continue through the published MIT preprocessor.
use crate::Error;
use swc_common::{FileName, SourceMap, sync::Lrc};
use swc_ecma_ast::*;
use swc_ecma_codegen::{Config, Emitter, Node, text_writer::JsWriter};
use swc_ecma_parser::{Parser, StringInput, Syntax, lexer::Lexer};
use ytdlp_ejs::builtin::polyfill::{INTL_POLYFILL, SETUP_CODE};

fn emit(node: &impl Node, map: &Lrc<SourceMap>) -> Result<String, Error> {
    let mut bytes = Vec::new();
    let mut emitter = Emitter {
        cfg: Config::default().with_minify(true),
        cm: map.clone(),
        comments: None,
        wr: JsWriter::new(map.clone(), "\n", &mut bytes, None),
    };
    node.emit_with(&mut emitter)
        .map_err(|_| Error::UnsupportedFormat)?;
    String::from_utf8(bytes).map_err(|_| Error::UnsupportedFormat)
}

fn scope(expression: &mut Expr) -> Option<&mut BlockStmt> {
    match expression {
        Expr::Fn(function) => function.function.body.as_mut(),
        Expr::Paren(paren) => scope(&mut paren.expr),
        Expr::Member(member) => scope(&mut member.obj),
        Expr::Call(call) => match &mut call.callee {
            Callee::Expr(expr) => scope(expr),
            _ => None,
        },
        _ => None,
    }
}

fn shared(function: &Function, map: &Lrc<SourceMap>) -> Result<bool, Error> {
    if function.is_async {
        return Ok(false);
    }
    let Some(body) = &function.body else {
        return Ok(false);
    };
    // This marker belongs to the URL-transform interface, not a minified name.
    // Serialize the parsed body so quotes/spacing do not affect recognition.
    Ok(emit(body, map)?.contains("\"alr\",\"yes\""))
}

pub(crate) fn prepare(source: &str) -> Result<String, Error> {
    let map: Lrc<SourceMap> = Default::default();
    let file = map.new_source_file(
        FileName::Custom("public-player.js".into()).into(),
        source.to_owned(),
    );
    let mut parser = Parser::new_from(Lexer::new(
        Syntax::Es(Default::default()),
        Default::default(),
        StringInput::from(&*file),
        None,
    ));
    let mut module = parser
        .parse_module()
        .map_err(|_| Error::UnsupportedFormat)?;
    let body = module
        .body
        .iter_mut()
        .rev()
        .find_map(|item| match item {
            ModuleItem::Stmt(Stmt::Expr(expr)) => scope(&mut expr.expr),
            _ => None,
        })
        .ok_or(Error::UnsupportedFormat)?;
    let mut candidates = Vec::new();
    let mut definitions = String::new();
    for statement in &mut body.stmts {
        if matches!(statement, Stmt::Decl(Decl::Var(var)) if var.decls.len()==1
            && matches!(&var.decls[0].name, Pat::Ident(name) if name.id.sym=="window")
            && matches!(var.decls[0].init.as_deref(), Some(Expr::This(_))))
        {
            // The isolated language runtime supplies its own inert window.
            continue;
        }
        match statement {
            Stmt::Decl(Decl::Fn(function)) if shared(&function.function, &map)? => {
                candidates.push(function.ident.sym.to_string());
            }
            Stmt::Decl(Decl::Var(var)) => {
                for decl in &mut var.decls {
                    if let Some(expr) = &mut decl.init
                        && let Expr::Fn(function) = expr.as_mut()
                        && let Pat::Ident(name) = &decl.name
                        && shared(&function.function, &map)?
                    {
                        candidates.push(name.id.sym.to_string());
                    }
                }
            }
            Stmt::Expr(expr) => {
                if let Expr::Assign(assign) = expr.expr.as_mut() {
                    if let Expr::Fn(function) = assign.right.as_ref()
                        && shared(&function.function, &map)?
                    {
                        candidates.push(emit(&assign.left, &map)?);
                    }
                    if let Expr::Bin(binary) = assign.right.as_ref()
                        && binary.op == BinaryOp::LogicalOr
                        && matches!(binary.left.as_ref(), Expr::This(_))
                        && matches!(binary.right.as_ref(), Expr::Ident(name) if name.sym == "self")
                    {
                        assign.right = binary.right.clone();
                    }
                }
            }
            _ => {}
        }
        let keep = match statement {
            Stmt::Expr(expr) => matches!(expr.expr.as_ref(), Expr::Assign(_) | Expr::Lit(_)),
            // Includes bounded table initialization loops whose declarations
            // are required by the transform functions (for example CRC data).
            _ => true,
        };
        if keep {
            let text = emit(statement, &map)?;
            if matches!(statement, Stmt::Decl(Decl::Fn(function)) if function.ident.sym == "g") {
                // Preserve source order instead of hoisting over the namespace.
                definitions.push_str("g=");
            }
            definitions.push_str(&text);
            definitions.push_str(";\n");
        }
    }
    if candidates.is_empty() {
        return ytdlp_ejs::preprocess_player(source).map_err(|_| Error::UnsupportedFormat);
    }
    if candidates.len() > 8 {
        return Err(Error::UnsupportedFormat);
    }
    let adapters = format!(
        r#"
const doremiTransforms=[{}];
function doremiTransform(kind,value) {{
  const answers=new Set();
  for (const factory of doremiTransforms) {{
    try {{
      const model=factory("https://www.youtube.com/watch?v=abcdefghijk","s",kind==="sig"?encodeURIComponent(value):undefined);
      if (kind==="n") model.set("n",value);
      const methods=Object.getOwnPropertyNames(Object.getPrototypeOf(model));
      const method=methods.find(key=>!["constructor","get","set","clone"].includes(key)&&typeof model[key]==="function");
      if (!method) continue;
      model[method]();
      let answer=model.get(kind==="sig"?"s":"n");
      if (kind==="sig"&&typeof answer==="string") answer=decodeURIComponent(answer);
      if (typeof answer==="string"&&answer.length&&answer!==value&&!answer.startsWith("enhanced_except_")) answers.add(answer);
    }} catch (_) {{}}
  }}
  if (answers.size!==1) throw new Error("Unsupported public player transform");
  return answers.values().next().value;
}}
_result.sig=value=>doremiTransform("sig",value);
_result.n=value=>doremiTransform("n",value);
"#,
        candidates.join(",")
    );
    Ok(format!(
        "{INTL_POLYFILL}\n{SETUP_CODE}\n{definitions}\n{adapters}"
    ))
}
