//! coclash 的 TUI 开发宏。
//!
//! 两个宏配合使用，注册表收敛在 `src/tui/windows/mod.rs` 一个文件：
//!
//! - `#[window]`：impl 级属性宏。窗口自身声明弹窗属性
//!   （`#[window(popup over Main)]`），校验 `on_open`/`draw` 契约，
//!   收集 `#[key(...)]` 方法，生成 `KEYS` 元数据与 `impl Window`（含按键分发）。
//! - `windows!`：函数式宏，展开生成 `Page` 枚举、`Windows` 结构体、
//!   导航/绘制分发（弹窗叠加顺序取自各窗口 `Window::meta`）与 `BINDINGS` 聚合。
//!
//! # 隐式约定（由宏推导，改名即改变语义）
//!
//! - `Page` 变体名与 `Windows` 字段名由窗口类型名**去掉 `Window` 后缀**推导：
//!   `MainWindow` → `Page::Main` / 字段 `main`（蛇形化）。
//! - `windows!` 里**第一个登记的窗口是初始页**（当前为 `MainWindow`）。
//! - 按键分发按声明顺序生成 match，**通配符按键（如 `KeyCode::Char(_)`）必须声明在
//!   最后**——它会吞掉其后所有同路径按键，顺序错误已做编译期强制校验。
use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2, TokenTree};
use quote::quote;
use syn::{
    parse::{Parse, ParseStream, Parser},
    parse_macro_input,
    spanned::Spanned,
    Error, Ident, ImplItem, ItemImpl, LitBool, LitStr, ReturnType, Token, Type,
};

// ==================== #[key(...)] 参数解析 ====================

/// `#[key(KeyCode::X, "描述", footer = true)]`
///
/// - 第一个参数：按键规格，既作 match 模式又作 `KeyCode` 表达式。
///   通配符（如 `KeyCode::Char(_)`）只能省略描述，否则报错。
/// - 第二个参数（可选）：帮助文案；**有描述才会进入 KEYS/BINDINGS**。
/// - `footer = true`（可选，默认 false）：是否显示在底部栏。
struct KeyAttr {
    spec: TokenStream2,
    desc: Option<LitStr>,
    footer: bool,
}

impl Parse for KeyAttr {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut spec = TokenStream2::new();
        while !input.is_empty() && !input.peek(Token![,]) {
            let tt: TokenTree = input.parse()?;
            spec.extend([tt]);
        }
        let mut desc = None;
        let mut footer = false;
        while input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }
            if input.peek(LitStr) {
                if desc.is_some() {
                    return Err(input.error("desc 只能出现一次"));
                }
                desc = Some(input.parse()?);
            } else {
                let ident: Ident = input.parse()?;
                if ident != "footer" {
                    return Err(Error::new(ident.span(), "期望 `footer = true/false`"));
                }
                input.parse::<Token![=]>()?;
                footer = input.parse::<LitBool>()?.value;
            }
        }
        Ok(Self { spec, desc, footer })
    }
}

/// 规格中是否含通配符 `_`（如 `KeyCode::Char(_)`），递归进入括号组
fn is_wildcard(spec: &TokenStream2) -> bool {
    fn has_underscore(tt: &TokenTree) -> bool {
        match tt {
            TokenTree::Ident(i) => i == "_",
            TokenTree::Group(g) => {
                g.stream().into_iter().any(|inner| has_underscore(&inner))
            }
            _ => false,
        }
    }
    spec.clone().into_iter().any(|tt| has_underscore(&tt))
}

/// 提取按键规格的变体名：`KeyCode::Char(_)` → `Char`（第一个括号分组前的最后一个 Ident）。
/// 无分组（如 `KeyCode::Up`）返回该 Ident 本身，仅用于同名比较。
fn variant_of(spec: &TokenStream2) -> Option<Ident> {
    let mut last_ident = None;
    for tt in spec.clone() {
        match tt {
            TokenTree::Ident(i) => last_ident = Some(i),
            TokenTree::Group(_) => break,
            _ => {}
        }
    }
    last_ident
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_key(input: &str) -> TokenStream2 {
        input.parse().unwrap()
    }

    #[test]
    fn wildcard_detection() {
        let tokens: TokenStream2 = parse_key("KeyCode::Char(_)");
        let dbg: Vec<String> = tokens
            .clone()
            .into_iter()
            .map(|tt| format!("{tt:?}"))
            .collect();
        assert!(is_wildcard(&tokens), "tokens: {dbg:?}");
        let tokens2: TokenStream2 = parse_key("KeyCode::Char('q')");
        assert!(!is_wildcard(&tokens2));
    }

    #[test]
    fn variant_extraction() {
        assert_eq!(
            variant_of(&parse_key("KeyCode::Char(_)")).map(|i| i.to_string()),
            Some("Char".to_string())
        );
        assert_eq!(
            variant_of(&parse_key("KeyCode::F(1)")).map(|i| i.to_string()),
            Some("F".to_string())
        );
        assert_eq!(
            variant_of(&parse_key("KeyCode::Up")).map(|i| i.to_string()),
            Some("Up".to_string())
        );
    }
}

// ==================== #[window] impl 级属性宏 ====================

/// `#[window]` 参数：裸标注 = 全屏页面；`#[window(popup over <Page>)]` = 弹窗。
struct WindowAttr {
    parent: Option<Ident>,
}

impl Parse for WindowAttr {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.is_empty() {
            return Ok(Self { parent: None });
        }
        let kw: Ident = input.parse()?;
        if kw != "popup" {
            return Err(Error::new(
                kw.span(),
                "期望 `#[window]` 或 `#[window(popup over <Page>)]`",
            ));
        }
        let over: Ident = input.parse()?;
        if over != "over" {
            return Err(Error::new(over.span(), "期望 `popup over <Page>`"));
        }
        let parent: Ident = input.parse()?;
        if !input.is_empty() {
            return Err(input.error("多余的 token"));
        }
        Ok(Self { parent: Some(parent) })
    }
}

/// 收集到的一条按键处理器
struct KeyEntry {
    ident: Ident,
    spec: TokenStream2,
    desc: Option<LitStr>,
    footer: bool,
    wildcard: bool,
}

/// 在 impl 上标注：声明窗口属性、校验契约、收集 `#[key]` 方法，
/// 生成 `KEYS` 常量与 `impl crate::tui::window::Window`。
///
/// 窗口固有方法约定（在 `#[window]` impl 内书写）：
/// - `pub fn new(manager: &Manager) -> Self`
/// - `pub fn on_open(&mut self)`
/// - `pub fn draw(&mut self, manager: &Manager, f: &mut Frame)`
///
/// 按键处理器签名约定：
/// - 普通按键：`fn(&mut self, manager: &Manager) -> Option<Page>`
/// - 通配符按键（如 `#[key(KeyCode::Char(_))]`，需要原始按键）：`fn(&mut self, manager: &Manager, key: KeyEvent) -> Option<Page>`
///
/// **通配符按键必须声明在最后**（编译期强制）：match 按声明顺序匹配，
/// 通配符会吞掉其后所有同路径的具体按键。
#[proc_macro_attribute]
pub fn window(attr: TokenStream, item: TokenStream) -> TokenStream {
    let original = parse_macro_input!(item as ItemImpl);
    let mut imp = original.clone();
    let mut errors: Vec<Error> = Vec::new();

    if imp.trait_.is_some() {
        errors.push(Error::new(
            imp.span(),
            "#[window] 只能作用于具体类型的 impl",
        ));
    }

    let window_attr: WindowAttr = match syn::parse::<WindowAttr>(attr) {
        Ok(a) => a,
        Err(e) => {
            errors.push(e);
            WindowAttr { parent: None }
        }
    };

    // 提取 on_open / draw（移入生成的 trait impl），其余方法留在原 impl
    let has_mut_self = |f: &syn::ImplItemFn| {
        matches!(f.sig.receiver(), Some(r) if r.reference.is_some() && r.mutability.is_some())
    };
    let mut on_open: Option<ImplItem> = None;
    let mut draw: Option<ImplItem> = None;
    let mut i = 0;
    while i < imp.items.len() {
        let matches_target = matches!(
            &imp.items[i],
            ImplItem::Fn(f)
                if (f.sig.ident == "on_open" && on_open.is_none())
                    || (f.sig.ident == "draw" && draw.is_none())
        );
        if matches_target {
            let ImplItem::Fn(f) = &imp.items[i] else {
                unreachable!()
            };
            if !has_mut_self(f) {
                errors.push(Error::new(
                    f.sig.span(),
                    "#[window] 的 on_open/draw 必须以 `&mut self` 作为第一个参数",
                ));
            }
            if f.sig.ident == "on_open" {
                on_open = Some(imp.items.remove(i));
            } else {
                draw = Some(imp.items.remove(i));
            }
        } else {
            i += 1;
        }
    }
    if on_open.is_none() {
        errors.push(Error::new(
            imp.span(),
            "#[window] 窗口缺少 `pub fn on_open(&mut self)`",
        ));
    }
    if draw.is_none() {
        errors.push(Error::new(
            imp.span(),
            "#[window] 窗口缺少 `pub fn draw(&mut self, manager: &Manager, f: &mut Frame)`",
        ));
    }

    let mut entries: Vec<KeyEntry> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for item in &mut imp.items {
        let ImplItem::Fn(method) = item else {
            continue;
        };
        let Some(pos) = method.attrs.iter().position(|a| a.path().is_ident("key")) else {
            continue;
        };
        let attr = method.attrs.remove(pos);
        let key_attr: KeyAttr = match attr.parse_args() {
            Ok(k) => k,
            Err(e) => {
                errors.push(e);
                continue;
            }
        };
        let wildcard = is_wildcard(&key_attr.spec);
        if wildcard && key_attr.desc.is_some() {
            errors.push(Error::new(
                attr.span(),
                "通配符按键（如 KeyCode::Char(_)）不能带描述文案，否则会刷屏帮助/底部栏；请省略 desc",
            ));
            continue;
        }

        let sig = &method.sig;
        match sig.receiver() {
            Some(r) if r.reference.is_some() && r.mutability.is_some() => {}
            _ => {
                errors.push(Error::new(sig.span(), "#[key] 方法必须以 `&mut self` 作为第一个参数"));
                continue;
            }
        }
        let expected = if wildcard { 3 } else { 2 };
        if sig.inputs.len() != expected {
            let hint = if wildcard {
                "签名：`fn(&mut self, manager: &Manager, key: KeyEvent) -> Option<Page>`"
            } else {
                "签名：`fn(&mut self, manager: &Manager) -> Option<Page>`"
            };
            errors.push(Error::new(
                sig.span(),
                format!("#[key] 方法参数数量不对，期望 {hint}"),
            ));
            continue;
        }
        let ret_is_option = match &sig.output {
            ReturnType::Type(_, ty) => {
                matches!(&**ty, Type::Path(p) if p.path.segments.last().is_some_and(|s| s.ident == "Option"))
            }
            _ => false,
        };
        if !ret_is_option {
            errors.push(Error::new(sig.span(), "#[key] 方法返回类型必须是 Option<Page>"));
            continue;
        }

        let spec_str = key_attr.spec.to_string();
        if !seen.insert(spec_str.clone()) {
            errors.push(Error::new(
                sig.ident.span(),
                format!("按键 `{spec_str}` 已在本窗口中重复绑定"),
            ));
            continue;
        }

        entries.push(KeyEntry {
            ident: sig.ident.clone(),
            spec: key_attr.spec,
            desc: key_attr.desc,
            footer: key_attr.footer,
            wildcard,
        });
    }

    // 通配符按键（如 `KeyCode::Char(_)`）在 match 中会吞掉其后**同变体**的具体按键
    // （如 `KeyCode::Char('q')`），必须声明在其之前；顺序错误编译期报错，
    // 防止新增具体按键后静默失效。
    for (i, e) in entries.iter().enumerate() {
        if !e.wildcard {
            continue;
        }
        let variant = variant_of(&e.spec);
        let Some(variant) = variant else { continue };
        if let Some(shadowed) = entries[i + 1..].iter().find(|later| {
            !later.wildcard && variant_of(&later.spec).is_some_and(|v| v == variant)
        }) {
            errors.push(Error::new(
                shadowed.ident.span(),
                format!(
                    "具体按键 `{}` 被其后的通配符 `{}` 吞掉：通配符必须声明在同变体具体按键之后",
                    shadowed.spec, e.spec
                ),
            ));
        }
    }

    if !errors.is_empty() {
        let errs = errors.iter().map(|e| e.to_compile_error());
        return quote! { #original #(#errs)* }.into();
    }

    let key_defs = entries
        .iter()
        .filter(|e| e.desc.is_some())
        .map(|e| {
            let spec = &e.spec;
            let desc = e.desc.as_ref().unwrap();
            let footer = e.footer;
            quote! {
                crate::tui::keymap::KeyDef { key: #spec, desc: Some(#desc), in_footer: #footer }
            }
        });
    let arms = entries.iter().map(|e| {
        let pat = &e.spec;
        let ident = &e.ident;
        if e.wildcard {
            quote! { #pat => self.#ident(app, key) }
        } else {
            quote! { #pat => self.#ident(app) }
        }
    });
    let generated_tokens = quote! {
        /// 按键元数据（由 #[window] 生成），帮助/底部栏的数据源。
        pub const KEYS: &[crate::tui::keymap::KeyDef] = &[
            #(#key_defs,)*
        ];
    };
    let parser = |input: ParseStream| {
        let mut items = Vec::new();
        while !input.is_empty() {
            items.push(input.parse::<ImplItem>()?);
        }
        Ok(items)
    };
    let generated: Vec<ImplItem> = match parser.parse2(generated_tokens) {
        Ok(items) => items,
        Err(e) => return e.to_compile_error().into(),
    };
    imp.items.extend(generated);

    let self_ty = &imp.self_ty;
    let parent_expr = match &window_attr.parent {
        Some(p) => quote! { Some(crate::tui::Page::#p) },
        None => quote! { None },
    };
    // 移入 trait impl 前，抹掉 visibility / async / unsafe 等修饰（trait 方法不允许）
    let to_trait_method = |mut item: ImplItem| {
        let ImplItem::Fn(f) = &mut item else {
            return item;
        };
        f.vis = syn::Visibility::Inherited;
        f.sig.constness = None;
        f.sig.asyncness = None;
        f.sig.unsafety = None;
        f.sig.abi = None;
        item
    };
    let on_open = on_open.map(to_trait_method);
    let draw = draw.map(to_trait_method);
    let trait_impl = quote! {
        impl crate::tui::window::Window for #self_ty {
            fn meta(&self) -> crate::tui::window::WindowMeta {
                crate::tui::window::WindowMeta { parent: #parent_expr }
            }

            #on_open

            #draw

            /// 按键分发（由 #[window] 生成）：声明顺序即匹配优先级，通配符请放在最后。
            fn handle_key(
                &mut self,
                app: &crate::manager::Manager,
                key: crossterm::event::KeyEvent,
            ) -> Option<crate::tui::Page> {
                match key.code {
                    #(#arms,)*
                    _ => None,
                }
            }
        }
    };

    quote! { #imp #trait_impl }.into()
}

// ==================== windows! 注册表宏 ====================

struct WindowEntry {
    ty: Ident,
    page: Ident,
    field: Ident,
}

impl Parse for WindowEntry {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let ty: Ident = input.parse()?;
        let ty_str = ty.to_string();
        let page_str = ty_str.strip_suffix("Window").ok_or_else(|| {
            Error::new(
                ty.span(),
                "窗口类型名必须以 `Window` 结尾，如 `MainWindow`",
            )
        })?;
        if page_str.is_empty() {
            return Err(Error::new(ty.span(), "窗口类型名不能只是 `Window`"));
        }
        let page = Ident::new(page_str, ty.span());
        let field = Ident::new(&to_snake_case(page_str), ty.span());
        Ok(Self { ty, page, field })
    }
}

struct WindowList {
    entries: Vec<WindowEntry>,
}

impl Parse for WindowList {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut entries = Vec::new();
        while !input.is_empty() {
            entries.push(input.parse()?);
            if input.is_empty() {
                break;
            }
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }
        }
        Ok(Self { entries })
    }
}

/// PascalCase → snake_case（用于从窗口类型名推导字段名）
fn to_snake_case(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 && !out.ends_with('_') {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// 注册表宏：在 `tui/windows/mod.rs` 调用一次，展开生成
/// `Page` 枚举、`Windows` 结构体、导航/绘制分发与 `BINDINGS` 聚合。
///
/// 弹窗属性（`popup over`）声明在各窗口的 `#[window]` 上，
/// 绘制叠加顺序通过 `Window::meta` 运行时读取。
///
/// 注意：**第一个登记的窗口是初始页**（当前为 `MainWindow`）；
/// `Page` 变体名与字段名由类型名去掉 `Window` 后缀推导，改名结构体即改名页面身份。
#[proc_macro]
pub fn windows(input: TokenStream) -> TokenStream {
    let list = parse_macro_input!(input as WindowList);
    let entries = &list.entries;

    if entries.is_empty() {
        return Error::new(Span::call_site(), "windows! 至少需要登记一个窗口")
            .to_compile_error()
            .into();
    }

    let variants = entries.iter().map(|e| &e.page);
    let uses = entries.iter().map(|e| {
        let field = &e.field;
        let ty = &e.ty;
        quote! { pub use #field::#ty; }
    });
    let fields = entries.iter().map(|e| {
        let field = &e.field;
        let ty = &e.ty;
        quote! { pub #field: #ty }
    });
    let inits = entries.iter().map(|e| {
        let field = &e.field;
        let ty = &e.ty;
        quote! { #field: #ty::new(app) }
    });
    let first = &entries[0].page;
    let open_arms = entries.iter().map(|e| {
        let page = &e.page;
        let field = &e.field;
        quote! { Page::#page => self.#field.on_open() }
    });
    let handle_arms = entries.iter().map(|e| {
        let page = &e.page;
        let field = &e.field;
        quote! { Page::#page => self.#field.handle_key(app, key) }
    });
    let draw_arms = entries.iter().map(|e| {
        let page = &e.page;
        let field = &e.field;
        let ty = &e.ty;
        quote! {
            Page::#page => {
                if let Some(parent) = #ty::meta(&self.#field).parent {
                    self.draw_page(parent, app, f);
                }
                self.#field.draw(app, f);
            }
        }
    });
    let binding_ext = entries.iter().map(|e| {
        let ty = &e.ty;
        let page = &e.page;
        quote! {
            bindings.extend(#ty::KEYS.iter().filter_map(|k| {
                k.desc.map(|desc| crate::tui::keymap::Binding {
                    mode: Page::#page,
                    key: k.key,
                    desc,
                    in_footer: k.in_footer,
                })
            }));
        }
    });

    quote! {
        use crate::tui::window::Window;

        /// 窗口页面身份（由 windows! 注册表生成）
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Page {
            #(#variants,)*
        }

        #(#uses)*

        /// 窗口管理器（由 windows! 注册表生成）：持有全部窗口，负责导航、按键分发与绘制。
        pub struct Windows {
            pub current: Page,
            #(#fields,)*
        }

        impl Windows {
            pub fn new(app: &crate::manager::Manager) -> Self {
                Self {
                    current: Page::#first,
                    #(#inits,)*
                }
            }

            /// 导航到某页；页面切换时触发对应窗口的 `on_open` 钩子
            pub fn open(&mut self, page: Page) {
                if page == self.current {
                    return;
                }
                self.current = page;
                match page {
                    #(#open_arms,)*
                }
            }

            /// 按键分发：窗口返回 `Some(page)` 表示请求导航
            pub fn handle_key(&mut self, app: &crate::manager::Manager, key: crossterm::event::KeyEvent) {
                let nav = match self.current {
                    #(#handle_arms,)*
                };
                if let Some(page) = nav {
                    self.open(page);
                }
            }

            /// 绘制某页：若该页是弹窗（`Window::meta` 声明了父页面），先递归画父页面再叠加自己
            fn draw_page(&mut self, page: Page, app: &crate::manager::Manager, f: &mut ratatui::Frame) {
                match page {
                    #(#draw_arms,)*
                }
            }

            /// 绘制当前页
            pub fn draw(&mut self, app: &crate::manager::Manager, f: &mut ratatui::Frame) {
                self.draw_page(self.current, app, f);
            }
        }

        /// 全部窗口按键聚合（由 windows! 注册表生成），帮助弹窗和底部栏据此自动生成。
        pub static BINDINGS: std::sync::LazyLock<Vec<crate::tui::keymap::Binding>> =
            std::sync::LazyLock::new(|| {
                let mut bindings = Vec::new();
                #(#binding_ext)*
                bindings
            });
    }
    .into()
}
