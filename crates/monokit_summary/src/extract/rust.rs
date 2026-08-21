//! Rust source extractor using `syn`.
//!
//! Walks the AST and emits [`PublicItem`]s for every `pub` item.
//! At [`Level::Index`] only names are included; at [`Level::Signature`]
//! full type signatures are added; at [`Level::Outline`] full docs are
//! included too.

use proc_macro2::Span;
use quote::quote;
use syn::spanned::Spanned as _;
use syn::visit::Visit;

use crate::types::ItemKind;
use crate::types::Level;
use crate::types::PublicItem;

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub(super) fn extract(source: &str, level: Level) -> anyhow::Result<Vec<PublicItem>> {
	let file = syn::parse_file(source)?;
	let mut visitor = RustVisitor {
		items: Vec::new(),
		level,
		parent_path: Vec::new(),
	};
	visitor.visit_file(&file);
	Ok(visitor.items)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract doc comments from `#[doc = "..."]` attributes.
fn doc_from_attrs(attrs: &[syn::Attribute], level: Level) -> (Option<String>, Option<String>) {
	let mut lines: Vec<String> = Vec::new();
	for attr in attrs {
		if !attr.meta.path().is_ident("doc") {
			continue;
		}
		let Ok(nv) = attr.meta.require_name_value() else {
			continue;
		};
		if let syn::Expr::Lit(syn::ExprLit {
			lit: syn::Lit::Str(s),
			..
		}) = &nv.value
		{
			lines.push(s.value());
		}
	}

	if lines.is_empty() {
		return (None, None);
	}

	let first = Some(lines[0].trim().to_string());
	let full = if level == Level::Outline {
		Some(lines.join("\n").trim_end().to_string())
	} else {
		None
	};
	(first, full)
}

/// Get the 1-based line number for a `Span`.
fn line_of(span: Span) -> u32 {
	span.start().line as u32
}

/// Render a `syn::Type` to a string using `quote!`.
fn ty_to_string(ty: &syn::Type) -> String {
	quote!(#ty).to_string()
}

/// Render a `syn::Signature` to a string using `quote!`.
fn sig_to_string(sig: &syn::Signature) -> String {
	quote!(#sig).to_string()
}

// ---------------------------------------------------------------------------
// Visitor
// ---------------------------------------------------------------------------

struct RustVisitor {
	items: Vec<PublicItem>,
	level: Level,
	parent_path: Vec<String>,
}

impl RustVisitor {
	fn is_pub(vis: &syn::Visibility) -> bool {
		matches!(
			vis,
			syn::Visibility::Public(_) | syn::Visibility::Restricted(_)
		)
	}

	/// Return None at Index level; otherwise a formatted signature string.
	fn maybe_sig(&self, s: String) -> Option<String> {
		match self.level {
			Level::Index => None,
			Level::Signature | Level::Outline => Some(s),
		}
	}
}

impl<'a> Visit<'a> for RustVisitor {
	// ----- Items -----

	fn visit_item_fn(&mut self, node: &'a syn::ItemFn) {
		if Self::is_pub(&node.vis) {
			let (first_doc, full_docs) = doc_from_attrs(&node.attrs, self.level);
			self.items.push(PublicItem {
				name: node.sig.ident.to_string(),
				kind: ItemKind::Function,
				line: line_of(node.sig.ident.span()),
				signature: self.maybe_sig(sig_to_string(&node.sig)),
				first_doc_line: first_doc,
				full_docs,
				parent_path: self.parent_path.clone(),
			});
		}
		syn::visit::visit_item_fn(self, node);
	}

	fn visit_item_struct(&mut self, node: &'a syn::ItemStruct) {
		if Self::is_pub(&node.vis) {
			let (first_doc, full_docs) = doc_from_attrs(&node.attrs, self.level);
			self.items.push(PublicItem {
				name: node.ident.to_string(),
				kind: ItemKind::Struct,
				line: line_of(node.ident.span()),
				signature: self.maybe_sig(format!("struct {}", node.ident)),
				first_doc_line: first_doc,
				full_docs,
				parent_path: self.parent_path.clone(),
			});
		}
	}

	fn visit_item_enum(&mut self, node: &'a syn::ItemEnum) {
		if Self::is_pub(&node.vis) {
			let (first_doc, full_docs) = doc_from_attrs(&node.attrs, self.level);
			self.items.push(PublicItem {
				name: node.ident.to_string(),
				kind: ItemKind::Enum,
				line: line_of(node.ident.span()),
				signature: self.maybe_sig(format!("enum {}", node.ident)),
				first_doc_line: first_doc,
				full_docs,
				parent_path: self.parent_path.clone(),
			});
		}
	}

	fn visit_item_trait(&mut self, node: &'a syn::ItemTrait) {
		if Self::is_pub(&node.vis) {
			let (first_doc, full_docs) = doc_from_attrs(&node.attrs, self.level);
			self.items.push(PublicItem {
				name: node.ident.to_string(),
				kind: ItemKind::Trait,
				line: line_of(node.ident.span()),
				signature: self.maybe_sig(format!("trait {}", node.ident)),
				first_doc_line: first_doc,
				full_docs,
				parent_path: self.parent_path.clone(),
			});
		}
	}

	fn visit_item_const(&mut self, node: &'a syn::ItemConst) {
		if Self::is_pub(&node.vis) {
			let (first_doc, full_docs) = doc_from_attrs(&node.attrs, self.level);
			self.items.push(PublicItem {
				name: node.ident.to_string(),
				kind: ItemKind::Const,
				line: line_of(node.ident.span()),
				signature: self.maybe_sig(format!(
					"const {}: {}",
					node.ident,
					ty_to_string(&node.ty)
				)),
				first_doc_line: first_doc,
				full_docs,
				parent_path: self.parent_path.clone(),
			});
		}
	}

	fn visit_item_static(&mut self, node: &'a syn::ItemStatic) {
		if Self::is_pub(&node.vis) {
			let (first_doc, full_docs) = doc_from_attrs(&node.attrs, self.level);
			self.items.push(PublicItem {
				name: node.ident.to_string(),
				kind: ItemKind::Static,
				line: line_of(node.ident.span()),
				signature: self.maybe_sig(format!(
					"static {}: {}",
					node.ident,
					ty_to_string(&node.ty)
				)),
				first_doc_line: first_doc,
				full_docs,
				parent_path: self.parent_path.clone(),
			});
		}
	}

	fn visit_item_type(&mut self, node: &'a syn::ItemType) {
		if Self::is_pub(&node.vis) {
			let (first_doc, full_docs) = doc_from_attrs(&node.attrs, self.level);
			self.items.push(PublicItem {
				name: node.ident.to_string(),
				kind: ItemKind::TypeAlias,
				line: line_of(node.ident.span()),
				signature: self.maybe_sig(format!("type {}", node.ident)),
				first_doc_line: first_doc,
				full_docs,
				parent_path: self.parent_path.clone(),
			});
		}
	}

	fn visit_item_mod(&mut self, node: &'a syn::ItemMod) {
		if Self::is_pub(&node.vis) {
			let (first_doc, full_docs) = doc_from_attrs(&node.attrs, self.level);
			self.items.push(PublicItem {
				name: node.ident.to_string(),
				kind: ItemKind::Module,
				line: line_of(node.ident.span()),
				signature: None,
				first_doc_line: first_doc,
				full_docs,
				parent_path: self.parent_path.clone(),
			});

			let saved = self.parent_path.clone();
			self.parent_path.push(node.ident.to_string());
			if let Some((_brace, content)) = &node.content {
				for item in content {
					self.visit_item(item);
				}
			}
			self.parent_path = saved;
		}
	}

	fn visit_item_macro(&mut self, node: &'a syn::ItemMacro) {
		if let Some(ident) = &node.ident {
			let (first_doc, full_docs) = doc_from_attrs(&node.attrs, self.level);
			self.items.push(PublicItem {
				name: ident.to_string(),
				kind: ItemKind::Macro,
				line: line_of(ident.span()),
				signature: self.maybe_sig(format!("macro_rules! {ident}")),
				first_doc_line: first_doc,
				full_docs,
				parent_path: self.parent_path.clone(),
			});
		}
	}

	fn visit_item_impl(&mut self, node: &'a syn::ItemImpl) {
		let trait_name = node
			.trait_
			.as_ref()
			.and_then(|(_, path, _)| path.segments.last().map(|s| s.ident.to_string()));

		if trait_name.is_some() {
			let name = trait_name.unwrap_or_else(|| "<trait>".to_string());
			let self_ty_str = ty_to_string(&node.self_ty);
			self.items.push(PublicItem {
				name: format!("impl {name} for {self_ty_str}"),
				kind: ItemKind::Impl,
				line: line_of(node.self_ty.span()),
				signature: self.maybe_sig(format!("impl {name} for {self_ty_str}")),
				first_doc_line: None,
				full_docs: None,
				parent_path: self.parent_path.clone(),
			});
		}

		// Visit associated items.
		let saved = self.parent_path.clone();
		if let syn::Type::Path(tp) = &*node.self_ty {
			if let Some(seg) = tp.path.segments.last() {
				self.parent_path.push(seg.ident.to_string());
				self.parent_path.push("impl".to_string());
			}
		}
		for item in &node.items {
			match item {
				syn::ImplItem::Fn(method) => {
					let (first_doc, full_docs) = doc_from_attrs(&method.attrs, self.level);
					self.items.push(PublicItem {
						name: method.sig.ident.to_string(),
						kind: ItemKind::Function,
						line: line_of(method.sig.ident.span()),
						signature: self.maybe_sig(sig_to_string(&method.sig)),
						first_doc_line: first_doc,
						full_docs,
						parent_path: self.parent_path.clone(),
					});
				}
				syn::ImplItem::Const(c) => {
					self.items.push(PublicItem {
						name: c.ident.to_string(),
						kind: ItemKind::Const,
						line: line_of(c.ident.span()),
						signature: self.maybe_sig(format!(
							"const {}: {}",
							c.ident,
							ty_to_string(&c.ty)
						)),
						first_doc_line: None,
						full_docs: None,
						parent_path: self.parent_path.clone(),
					});
				}
				syn::ImplItem::Type(t) => {
					self.items.push(PublicItem {
						name: t.ident.to_string(),
						kind: ItemKind::TypeAlias,
						line: line_of(t.ident.span()),
						signature: self.maybe_sig(format!("type {}", t.ident)),
						first_doc_line: None,
						full_docs: None,
						parent_path: self.parent_path.clone(),
					});
				}
				_ => {}
			}
		}
		self.parent_path = saved;
	}
}
