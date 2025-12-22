//! LSP-powered tools using tree-sitter for code understanding
//!
//! These tools provide IDE-like code intelligence:
//! - Go to definition
//! - Find all references  
//! - List symbols (functions, classes, types)
//! - Get function signatures
//! - Trace call hierarchy

use super::{Tool, ToolResult};
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tree_sitter::{Language, Parser, Query, QueryCursor};

/// Symbol information extracted from code
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub signature: Option<String>,
    pub doc_comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SymbolKind {
    Function,
    Method,
    Class,
    Struct,
    Interface,
    Trait,
    Enum,
    Constant,
    Variable,
    Module,
    Type,
    Field,
    Property,
    Import,
}

impl std::fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SymbolKind::Function => write!(f, "fn"),
            SymbolKind::Method => write!(f, "method"),
            SymbolKind::Class => write!(f, "class"),
            SymbolKind::Struct => write!(f, "struct"),
            SymbolKind::Interface => write!(f, "interface"),
            SymbolKind::Trait => write!(f, "trait"),
            SymbolKind::Enum => write!(f, "enum"),
            SymbolKind::Constant => write!(f, "const"),
            SymbolKind::Variable => write!(f, "var"),
            SymbolKind::Module => write!(f, "mod"),
            SymbolKind::Type => write!(f, "type"),
            SymbolKind::Field => write!(f, "field"),
            SymbolKind::Property => write!(f, "prop"),
            SymbolKind::Import => write!(f, "import"),
        }
    }
}

/// Code analyzer using tree-sitter
pub struct CodeAnalyzer {
    working_dir: PathBuf,
}

impl CodeAnalyzer {
    pub fn new(working_dir: PathBuf) -> Self {
        Self { working_dir }
    }

    /// Get the tree-sitter language for a file extension
    fn get_language(extension: &str) -> Option<Language> {
        match extension {
            "rs" => Some(tree_sitter_rust::language()),
            "ts" | "tsx" => Some(tree_sitter_typescript::language_typescript()),
            "js" | "jsx" | "mjs" => Some(tree_sitter_javascript::language()),
            "py" => Some(tree_sitter_python::language()),
            "go" => Some(tree_sitter_go::language()),
            "json" => Some(tree_sitter_json::language()),
            _ => None,
        }
    }

    /// Get tree-sitter query for extracting symbols based on language
    fn get_symbols_query(extension: &str) -> Option<&'static str> {
        match extension {
            "rs" => Some(
                r#"
                (function_item name: (identifier) @fn.name) @fn.def
                (impl_item type: (type_identifier) @impl.name) @impl.def
                (struct_item name: (type_identifier) @struct.name) @struct.def
                (enum_item name: (type_identifier) @enum.name) @enum.def
                (trait_item name: (type_identifier) @trait.name) @trait.def
                (mod_item name: (identifier) @mod.name) @mod.def
                (const_item name: (identifier) @const.name) @const.def
                (static_item name: (identifier) @static.name) @static.def
                (type_item name: (type_identifier) @type.name) @type.def
            "#,
            ),
            "ts" | "tsx" | "js" | "jsx" | "mjs" => Some(
                r#"
                (function_declaration name: (identifier) @fn.name) @fn.def
                (class_declaration name: (identifier) @class.name) @class.def
                (interface_declaration name: (identifier) @interface.name) @interface.def
                (type_alias_declaration name: (type_identifier) @type.name) @type.def
                (enum_declaration name: (identifier) @enum.name) @enum.def
                (method_definition name: (property_identifier) @method.name) @method.def
                (lexical_declaration (variable_declarator name: (identifier) @var.name)) @var.def
            "#,
            ),
            "py" => Some(
                r#"
                (function_definition name: (identifier) @fn.name) @fn.def
                (class_definition name: (identifier) @class.name) @class.def
            "#,
            ),
            "go" => Some(
                r#"
                (function_declaration name: (identifier) @fn.name) @fn.def
                (method_declaration name: (field_identifier) @method.name) @method.def
                (type_declaration (type_spec name: (type_identifier) @type.name)) @type.def
            "#,
            ),
            _ => None,
        }
    }

    /// Parse a file and extract symbols
    pub fn extract_symbols(&self, file_path: &Path) -> Result<Vec<Symbol>> {
        let extension = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

        let language = Self::get_language(extension)
            .ok_or_else(|| anyhow::anyhow!("Unsupported language: {}", extension))?;

        let query_str = Self::get_symbols_query(extension)
            .ok_or_else(|| anyhow::anyhow!("No query for language: {}", extension))?;

        let content = fs::read_to_string(file_path)?;
        let mut parser = Parser::new();
        parser.set_language(&language)?;

        let tree = parser
            .parse(&content, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse file"))?;

        let query = Query::new(&language, query_str)?;
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(&query, tree.root_node(), content.as_bytes());

        let mut symbols = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        for m in matches {
            let mut name = String::new();
            let mut kind = SymbolKind::Function;
            let mut start_line = 0;
            let mut start_col = 0;
            let mut end_line = 0;
            let mut signature = None;

            for capture in m.captures {
                let capture_name = query.capture_names()[capture.index as usize];
                let node = capture.node;
                let text = node.utf8_text(content.as_bytes()).unwrap_or("");

                if capture_name.ends_with(".name") {
                    name = text.to_string();
                    kind = match capture_name {
                        "fn.name" => SymbolKind::Function,
                        "method.name" => SymbolKind::Method,
                        "class.name" => SymbolKind::Class,
                        "struct.name" => SymbolKind::Struct,
                        "interface.name" => SymbolKind::Interface,
                        "trait.name" => SymbolKind::Trait,
                        "enum.name" => SymbolKind::Enum,
                        "const.name" | "static.name" => SymbolKind::Constant,
                        "var.name" => SymbolKind::Variable,
                        "mod.name" => SymbolKind::Module,
                        "type.name" | "impl.name" => SymbolKind::Type,
                        _ => SymbolKind::Function,
                    };
                } else if capture_name.ends_with(".def") {
                    start_line = node.start_position().row;
                    start_col = node.start_position().column;
                    end_line = node.end_position().row;

                    // Extract first line as signature
                    if start_line < lines.len() {
                        signature = Some(lines[start_line].trim().to_string());
                    }
                }
            }

            if !name.is_empty() {
                symbols.push(Symbol {
                    name,
                    kind,
                    file: file_path.display().to_string(),
                    line: start_line + 1, // 1-indexed
                    column: start_col + 1,
                    end_line: end_line + 1,
                    signature,
                    doc_comment: self.extract_doc_comment(&lines, start_line),
                });
            }
        }

        Ok(symbols)
    }

    /// Extract doc comment above a symbol
    fn extract_doc_comment(&self, lines: &[&str], start_line: usize) -> Option<String> {
        if start_line == 0 {
            return None;
        }

        let mut comments = Vec::new();
        let mut line_idx = start_line.saturating_sub(1);

        while line_idx > 0 {
            let line = lines[line_idx].trim();
            if line.starts_with("///") || line.starts_with("//!") {
                comments.push(
                    line.trim_start_matches("///")
                        .trim_start_matches("//!")
                        .trim(),
                );
            } else if line.starts_with("#") && !line.starts_with("#[") {
                // Python docstring indicator
                comments.push(line.trim_start_matches('#').trim());
            } else if !line.is_empty() && !line.starts_with("#[") && !line.starts_with("@") {
                break;
            }
            if line_idx == 0 {
                break;
            }
            line_idx -= 1;
        }

        if comments.is_empty() {
            None
        } else {
            comments.reverse();
            Some(comments.join(" "))
        }
    }

    /// Find definition of a symbol by name
    pub fn find_definition(
        &self,
        symbol_name: &str,
        search_files: &[PathBuf],
    ) -> Result<Vec<Symbol>> {
        let mut results = Vec::new();

        for file in search_files {
            if let Ok(symbols) = self.extract_symbols(file) {
                for symbol in symbols {
                    if symbol.name == symbol_name {
                        results.push(symbol);
                    }
                }
            }
        }

        Ok(results)
    }

    /// Find all references to a symbol (uses grep-like search + validation)
    pub fn find_references(
        &self,
        symbol_name: &str,
        search_files: &[PathBuf],
    ) -> Result<Vec<(String, usize, String)>> {
        let mut results = Vec::new();

        for file in search_files {
            if let Ok(content) = fs::read_to_string(file) {
                for (line_num, line) in content.lines().enumerate() {
                    // Simple word boundary check
                    if line.contains(symbol_name) {
                        // Verify it's a word boundary (not part of another identifier)
                        let pattern = format!(r"\b{}\b", regex::escape(symbol_name));
                        if let Ok(re) = regex::Regex::new(&pattern) {
                            if re.is_match(line) {
                                results.push((
                                    file.display().to_string(),
                                    line_num + 1,
                                    line.trim().to_string(),
                                ));
                            }
                        }
                    }
                }
            }
        }

        Ok(results)
    }

    /// Get files to search based on extension
    pub fn get_searchable_files(&self, extensions: Option<&[&str]>) -> Vec<PathBuf> {
        let mut files = Vec::new();
        let walker = walkdir::WalkDir::new(&self.working_dir)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_str().unwrap_or("");
                !name.starts_with('.')
                    && name != "node_modules"
                    && name != "target"
                    && name != "dist"
                    && name != "build"
                    && name != "__pycache__"
                    && name != "vendor"
            });

        for entry in walker.flatten() {
            if entry.file_type().is_file() {
                let path = entry.path();
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    let should_include =
                        extensions
                            .map(|exts| exts.contains(&ext))
                            .unwrap_or_else(|| {
                                matches!(
                                    ext,
                                    "rs" | "ts"
                                        | "tsx"
                                        | "js"
                                        | "jsx"
                                        | "py"
                                        | "go"
                                        | "java"
                                        | "c"
                                        | "cpp"
                                        | "h"
                                        | "hpp"
                                )
                            });
                    if should_include {
                        files.push(path.to_path_buf());
                    }
                }
            }
        }

        files
    }
}

// ========== TOOLS ==========

/// Tool: List all symbols in a file or directory
pub struct ListSymbolsTool {
    analyzer: CodeAnalyzer,
}

impl ListSymbolsTool {
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            analyzer: CodeAnalyzer::new(working_dir),
        }
    }
}

#[async_trait]
impl Tool for ListSymbolsTool {
    fn name(&self) -> &str {
        "list_symbols"
    }

    fn description(&self) -> &str {
        "List all symbols (functions, classes, types, etc.) in a file or directory. \
         Use this to understand code structure and find specific definitions. \
         Returns symbol names, types, locations, and signatures."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "File or directory path to analyze"
                },
                "kind": {
                    "type": "string",
                    "description": "Filter by symbol kind: function, class, struct, trait, enum, type, const, module",
                    "enum": ["function", "class", "struct", "trait", "enum", "type", "const", "module", "all"]
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let path = params["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'path' parameter"))?;
        let kind_filter = params["kind"].as_str();

        let target_path = self.analyzer.working_dir.join(path);

        let files = if target_path.is_file() {
            vec![target_path]
        } else if target_path.is_dir() {
            self.analyzer
                .get_searchable_files(None)
                .into_iter()
                .filter(|f| f.starts_with(&target_path))
                .collect()
        } else {
            return Ok(ToolResult::error(format!("Path not found: {}", path)));
        };

        let mut all_symbols = Vec::new();
        for file in files {
            if let Ok(symbols) = self.analyzer.extract_symbols(&file) {
                all_symbols.extend(symbols);
            }
        }

        // Filter by kind if specified
        if let Some(kind) = kind_filter {
            if kind != "all" {
                all_symbols.retain(|s| {
                    let kind_str = format!("{}", s.kind).to_lowercase();
                    kind_str == kind
                        || matches!(
                            (kind, &s.kind),
                            ("function", SymbolKind::Function)
                                | ("function", SymbolKind::Method)
                                | ("class", SymbolKind::Class)
                                | ("class", SymbolKind::Struct)
                        )
                });
            }
        }

        if all_symbols.is_empty() {
            return Ok(ToolResult::success("No symbols found"));
        }

        // Format output
        let mut output = format!("Found {} symbols:\n\n", all_symbols.len());

        // Group by file
        let mut by_file: HashMap<String, Vec<&Symbol>> = HashMap::new();
        for symbol in &all_symbols {
            by_file.entry(symbol.file.clone()).or_default().push(symbol);
        }

        for (file, symbols) in by_file {
            let rel_path = PathBuf::from(&file)
                .strip_prefix(&self.analyzer.working_dir)
                .map(|p| p.display().to_string())
                .unwrap_or(file);

            output.push_str(&format!("## {}\n", rel_path));
            for s in symbols {
                output.push_str(&format!(
                    "  {:8} {:30} L{}-{}\n",
                    format!("[{}]", s.kind),
                    s.name,
                    s.line,
                    s.end_line
                ));
                if let Some(sig) = &s.signature {
                    output.push_str(&format!("           {}\n", sig));
                }
            }
            output.push('\n');
        }

        Ok(ToolResult::success(output))
    }
}

/// Tool: Go to definition of a symbol
pub struct GoToDefinitionTool {
    analyzer: CodeAnalyzer,
}

impl GoToDefinitionTool {
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            analyzer: CodeAnalyzer::new(working_dir),
        }
    }
}

#[async_trait]
impl Tool for GoToDefinitionTool {
    fn name(&self) -> &str {
        "go_to_definition"
    }

    fn description(&self) -> &str {
        "Find the definition of a function, class, type, or variable. \
         Returns the exact file and line where the symbol is defined, \
         along with its signature and documentation."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "symbol": {
                    "type": "string",
                    "description": "Name of the symbol to find (function, class, type, etc.)"
                },
                "file_hint": {
                    "type": "string",
                    "description": "Optional: file where the symbol is used (helps narrow search)"
                }
            },
            "required": ["symbol"]
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let symbol = params["symbol"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'symbol' parameter"))?;

        let files = self.analyzer.get_searchable_files(None);
        let definitions = self.analyzer.find_definition(symbol, &files)?;

        if definitions.is_empty() {
            return Ok(ToolResult::success(format!(
                "No definition found for '{}'. Try:\n\
                 - Check spelling\n\
                 - Use list_symbols to see available symbols\n\
                 - The symbol might be from an external library",
                symbol
            )));
        }

        let mut output = format!(
            "Found {} definition(s) for '{}':\n\n",
            definitions.len(),
            symbol
        );

        for def in &definitions {
            let rel_path = PathBuf::from(&def.file)
                .strip_prefix(&self.analyzer.working_dir)
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| def.file.clone());

            output.push_str(&format!(
                "📍 **{}** ({})\n\
                   File: {}\n\
                   Line: {}-{}\n",
                def.name, def.kind, rel_path, def.line, def.end_line
            ));

            if let Some(sig) = &def.signature {
                output.push_str(&format!("   Signature: `{}`\n", sig));
            }
            if let Some(doc) = &def.doc_comment {
                output.push_str(&format!("   Doc: {}\n", doc));
            }
            output.push('\n');
        }

        Ok(ToolResult::success(output))
    }
}

/// Tool: Find all references to a symbol
pub struct FindAllReferencesTool {
    analyzer: CodeAnalyzer,
}

impl FindAllReferencesTool {
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            analyzer: CodeAnalyzer::new(working_dir),
        }
    }
}

#[async_trait]
impl Tool for FindAllReferencesTool {
    fn name(&self) -> &str {
        "find_all_references"
    }

    fn description(&self) -> &str {
        "Find all usages of a function, class, type, or variable across the codebase. \
         More accurate than grep as it uses word boundaries. \
         Use this before refactoring to understand impact."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "symbol": {
                    "type": "string",
                    "description": "Name of the symbol to find references for"
                },
                "include_definition": {
                    "type": "boolean",
                    "description": "Include the definition location (default: true)"
                }
            },
            "required": ["symbol"]
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let symbol = params["symbol"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'symbol' parameter"))?;
        let include_def = params["include_definition"].as_bool().unwrap_or(true);

        let files = self.analyzer.get_searchable_files(None);

        // Find definition first
        let definitions = if include_def {
            self.analyzer.find_definition(symbol, &files)?
        } else {
            vec![]
        };

        // Find all references
        let references = self.analyzer.find_references(symbol, &files)?;

        if references.is_empty() && definitions.is_empty() {
            return Ok(ToolResult::success(format!(
                "No references found for '{}'. The symbol might not exist or be from an external library.",
                symbol
            )));
        }

        let mut output = String::new();

        // Show definition
        if !definitions.is_empty() {
            output.push_str("## Definition\n\n");
            for def in &definitions {
                let rel_path = PathBuf::from(&def.file)
                    .strip_prefix(&self.analyzer.working_dir)
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| def.file.clone());
                output.push_str(&format!(
                    "  📍 {}:{} [{}]\n     {}\n\n",
                    rel_path,
                    def.line,
                    def.kind,
                    def.signature.as_deref().unwrap_or("")
                ));
            }
        }

        // Show references
        output.push_str(&format!("## References ({} found)\n\n", references.len()));

        // Group by file
        let mut by_file: HashMap<String, Vec<(usize, String)>> = HashMap::new();
        for (file, line, content) in references {
            by_file.entry(file).or_default().push((line, content));
        }

        for (file, refs) in by_file {
            let rel_path = PathBuf::from(&file)
                .strip_prefix(&self.analyzer.working_dir)
                .map(|p| p.display().to_string())
                .unwrap_or(file);

            output.push_str(&format!("### {}\n", rel_path));
            for (line, content) in refs {
                output.push_str(&format!("  L{}: {}\n", line, content));
            }
            output.push('\n');
        }

        Ok(ToolResult::success(output))
    }
}

/// Tool: Get call hierarchy (who calls what)
pub struct CallHierarchyTool {
    analyzer: CodeAnalyzer,
}

impl CallHierarchyTool {
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            analyzer: CodeAnalyzer::new(working_dir),
        }
    }
}

#[async_trait]
impl Tool for CallHierarchyTool {
    fn name(&self) -> &str {
        "call_hierarchy"
    }

    fn description(&self) -> &str {
        "Trace the call hierarchy of a function. Shows what functions call it (callers) \
         and what functions it calls (callees). Essential for understanding code flow \
         and impact analysis."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "function": {
                    "type": "string",
                    "description": "Name of the function to analyze"
                },
                "direction": {
                    "type": "string",
                    "description": "Direction: 'incoming' (who calls it), 'outgoing' (what it calls), 'both'",
                    "enum": ["incoming", "outgoing", "both"]
                }
            },
            "required": ["function"]
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let function = params["function"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'function' parameter"))?;
        let direction = params["direction"].as_str().unwrap_or("both");

        let files = self.analyzer.get_searchable_files(None);

        // Find the function definition
        let definitions = self.analyzer.find_definition(function, &files)?;

        if definitions.is_empty() {
            return Ok(ToolResult::success(format!(
                "Function '{}' not found. Use list_symbols to see available functions.",
                function
            )));
        }

        let mut output = format!("# Call Hierarchy for `{}`\n\n", function);

        // Find incoming calls (who calls this function)
        if direction == "incoming" || direction == "both" {
            output.push_str("## ⬅️ Incoming (Callers)\n\n");
            let callers = self.analyzer.find_references(function, &files)?;

            if callers.is_empty() {
                output.push_str("  No callers found (might be entry point or exported)\n\n");
            } else {
                for (file, line, content) in &callers {
                    let rel_path = PathBuf::from(file)
                        .strip_prefix(&self.analyzer.working_dir)
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|_| file.clone());
                    output.push_str(&format!("  {}:{}\n    {}\n\n", rel_path, line, content));
                }
            }
        }

        // Find outgoing calls (what does this function call)
        if direction == "outgoing" || direction == "both" {
            output.push_str("## ➡️ Outgoing (Callees)\n\n");

            // Read the function body and find function calls
            if let Some(def) = definitions.first() {
                if let Ok(content) = fs::read_to_string(&def.file) {
                    let lines: Vec<&str> = content.lines().collect();
                    let start = def.line.saturating_sub(1);
                    let end = def.end_line.min(lines.len());

                    // Simple heuristic: find identifiers followed by (
                    let func_body = lines[start..end].join("\n");
                    static CALL_PATTERN: once_cell::sync::Lazy<regex::Regex> =
                        once_cell::sync::Lazy::new(|| {
                            regex::Regex::new(r"(\w+)\s*\(").expect("valid regex")
                        });
                    let call_pattern = &*CALL_PATTERN;

                    let mut calls: Vec<String> = call_pattern
                        .captures_iter(&func_body)
                        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
                        .filter(|name| {
                            !["if", "while", "for", "match", "return", function]
                                .contains(&name.as_str())
                        })
                        .collect();

                    calls.sort();
                    calls.dedup();

                    if calls.is_empty() {
                        output.push_str("  No outgoing calls found\n\n");
                    } else {
                        for call in calls {
                            // Try to find where this call is defined
                            if let Ok(defs) = self.analyzer.find_definition(&call, &files) {
                                if let Some(d) = defs.first() {
                                    let rel_path = PathBuf::from(&d.file)
                                        .strip_prefix(&self.analyzer.working_dir)
                                        .map(|p| p.display().to_string())
                                        .unwrap_or_else(|_| d.file.clone());
                                    output.push_str(&format!(
                                        "  `{}` → {}:{}\n",
                                        call, rel_path, d.line
                                    ));
                                } else {
                                    output.push_str(&format!("  `{}` (external/stdlib)\n", call));
                                }
                            } else {
                                output.push_str(&format!("  `{}` (external/stdlib)\n", call));
                            }
                        }
                    }
                }
            }
        }

        Ok(ToolResult::success(output))
    }
}

/// Tool: Get signature/type info for a symbol
pub struct GetSignatureTool {
    analyzer: CodeAnalyzer,
}

impl GetSignatureTool {
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            analyzer: CodeAnalyzer::new(working_dir),
        }
    }
}

#[async_trait]
impl Tool for GetSignatureTool {
    fn name(&self) -> &str {
        "get_signature"
    }

    fn description(&self) -> &str {
        "Get the signature and type information for a function, method, or type. \
         Shows parameters, return type, and documentation. \
         Use this to understand API contracts before making changes."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "symbol": {
                    "type": "string",
                    "description": "Name of the function, method, or type"
                }
            },
            "required": ["symbol"]
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let symbol = params["symbol"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'symbol' parameter"))?;

        let files = self.analyzer.get_searchable_files(None);
        let definitions = self.analyzer.find_definition(symbol, &files)?;

        if definitions.is_empty() {
            return Ok(ToolResult::success(format!(
                "Symbol '{}' not found. Use list_symbols to see available symbols.",
                symbol
            )));
        }

        let mut output = String::new();

        for def in &definitions {
            let rel_path = PathBuf::from(&def.file)
                .strip_prefix(&self.analyzer.working_dir)
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| def.file.clone());

            output.push_str(&format!("# {} ({})\n\n", def.name, def.kind));
            output.push_str(&format!("📁 {}\n", rel_path));
            output.push_str(&format!("📍 Lines {}-{}\n\n", def.line, def.end_line));

            if let Some(sig) = &def.signature {
                output.push_str("## Signature\n```\n");
                output.push_str(sig);
                output.push_str("\n```\n\n");
            }

            if let Some(doc) = &def.doc_comment {
                output.push_str("## Documentation\n");
                output.push_str(doc);
                output.push_str("\n\n");
            }

            // Read more context for fuller signature
            if let Ok(content) = fs::read_to_string(&def.file) {
                let lines: Vec<&str> = content.lines().collect();
                let start = def.line.saturating_sub(1);
                let end = (start + 10).min(lines.len()); // First 10 lines

                output.push_str("## Context\n```\n");
                for line in &lines[start..end] {
                    output.push_str(line);
                    output.push('\n');
                }
                output.push_str("```\n");
            }
        }

        Ok(ToolResult::success(output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_symbol_extraction() {
        let cwd = env::current_dir().unwrap();
        let analyzer = CodeAnalyzer::new(cwd);

        // Test on this file itself
        let this_file = PathBuf::from(file!());
        if this_file.exists() {
            let symbols = analyzer.extract_symbols(&this_file).unwrap();
            assert!(!symbols.is_empty());
        }
    }
}
