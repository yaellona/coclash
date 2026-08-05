//! 过程宏：以 C# 特性（Attribute）的方式注册窗口（页面）和按键绑定。
//!
//! 三个属性宏：
//! - `#[window(name = "main")]`：主窗口，生成 `impl Window`
//! - `#[popup(name = "url_input")]`：弹窗，生成 `impl Window + impl Popup`
//! - `#[windows(base = main, popups = (url_input, ...))]`：挂在 `pub mod pages;`
//!   上，一处清单生成 `Windows` 容器（结构体 + 构造 + 分发 + 绘制）
//!
//! `#[window]`/`#[popup]` 挂在窗口 struct 的 impl 块上，块内方法挂标记属性：
//! - `#[key(KeyCode::Esc, desc = "取消")]`：按键处理器，可带 receiver
//!   （`fn(&mut self, m: &mut Manager)`，有状态）或不带
//!   （`fn(m: &mut Manager)`，主窗口无状态用）
//! - `#[fallback]`：handle_key 未命中时的兜底 `fn(&mut self, m, key)` 或 `fn(m, key)`
//! - `#[on_open]`：弹窗打开钩子（仅 `#[popup]` 可用）
//! - `#[render]`：绘制方法 `fn(&mut self, m, f)` 或 `fn(m, f)`
//!
//! 宏展开为：ID 常量 `X: WindowId`、元数据表 `X_BINDINGS`（Binding 无 run，纯数据）、
//! `impl Window`（handle_key match + draw）、弹窗另有 `impl Popup`。
//!
//! `#[key(...)]` 参数：
//! - 第一个位置参数为按键：字符字面量 `'q'` 自动转为 `KeyCode::Char('q')`，
//!   其他写法原样作为表达式（如 `KeyCode::Up`）
//! - `desc = "..."`：帮助文案，默认空字符串（不展示）
//! - `footer`：标记位，出现在底部栏
//!
//! `#[windows]` 要求窗口 struct 名为 `<PascalCase(name)>Window`，位于 `pages::<name>`
//! 模块，且有 `pub(crate) fn new(ctx: &WindowCtx) -> Self`。
//!
//! 生成代码要求展开处作用域内有 `Binding`、`WindowId`、`Window`、`Popup`、
//! `WindowCtx`、`Manager`、`Frame`、`KeyCode`、`LazyLock`。

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    parse::{Parse, ParseStream},
    parse2, spanned::Spanned, Error, Expr, Ident, ImplItem, ItemImpl, ItemStruct, Lit, LitStr,
    Meta, Result, Token,
};

struct WindowArgs {
    name: LitStr,
}

impl Parse for WindowArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.is_empty() {
            return Err(Error::new(
                proc_macro2::Span::call_site(),
                "缺少 `name` 参数，例如 #[window(name = \"main\")] 或 #[popup(name = \"url_input\")]",
            ));
        }
        let ident: Ident = input.parse()?;
        if ident != "name" {
            return Err(Error::new(
                ident.span(),
                format!("未知参数 `{ident}`，可用：name"),
            ));
        }
        input.parse::<Token![=]>()?;
        let name: LitStr = input.parse()?;
        if !input.is_empty() {
            input.parse::<Token![,]>()?;
        }
        if !input.is_empty() {
            return Err(input.error("多余的参数"));
        }
        Ok(WindowArgs { name })
    }
}

/// `#[window(name = "...")]` 属性宏：主窗口，生成 ID 常量 + 元数据表 + `impl Window`。
#[proc_macro_attribute]
pub fn window(attr: TokenStream, item: TokenStream) -> TokenStream {
    window_like_impl(attr.into(), item.into(), false)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

/// `#[popup(name = "...")]` 属性宏：弹窗，生成 ID 常量 + 元数据表 +
/// `impl Window + impl Popup`（支持 `#[on_open]` 钩子）。
#[proc_macro_attribute]
pub fn popup(attr: TokenStream, item: TokenStream) -> TokenStream {
    window_like_impl(attr.into(), item.into(), true)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

fn window_like_impl(attr: TokenStream2, item: TokenStream2, is_popup: bool) -> Result<TokenStream2> {
    let args: WindowArgs = parse2(attr)?;
    let name_str = args.name.value();
    let base = format_ident!("{}", name_str.to_uppercase());
    let table_name = format_ident!("{}_BINDINGS", name_str.to_uppercase());

    let mut impl_block: ItemImpl = parse2(item)?;
    if impl_block.trait_.is_some() {
        return Err(Error::new_spanned(
            &impl_block,
            "#[window]/#[popup] 只能用在固有 impl 块上",
        ));
    }

    let mut bindings = Vec::new();
    let mut key_arms = Vec::new();
    let mut draw: Option<(Ident, bool)> = None;
    let mut fallback: Option<(Ident, bool)> = None;
    let mut on_open: Option<(Ident, bool)> = None;

    for item in &mut impl_block.items {
        let ImplItem::Fn(method) = item else {
            continue;
        };
        let method_name = method.sig.ident.clone();
        let has_receiver = method.sig.receiver().is_some();

        if method.attrs.iter().any(|a| a.path().is_ident("render")) {
            if draw.is_some() {
                return Err(Error::new_spanned(
                    &method_name,
                    "每个窗口只能有一个 #[render] 方法",
                ));
            }
            draw = Some((method_name, has_receiver));
            method.attrs.retain(|a| !a.path().is_ident("render"));
            continue;
        }
        if method.attrs.iter().any(|a| a.path().is_ident("on_open")) {
            if !is_popup {
                return Err(Error::new_spanned(
                    &method_name,
                    "#[on_open] 只能用于 #[popup]",
                ));
            }
            if on_open.is_some() {
                return Err(Error::new_spanned(&method_name, "重复的 #[on_open] 方法"));
            }
            on_open = Some((method_name, has_receiver));
            method.attrs.retain(|a| !a.path().is_ident("on_open"));
            continue;
        }
        if method.attrs.iter().any(|a| a.path().is_ident("fallback")) {
            if fallback.is_some() {
                return Err(Error::new_spanned(&method_name, "重复的 #[fallback] 方法"));
            }
            fallback = Some((method_name, has_receiver));
            method.attrs.retain(|a| !a.path().is_ident("fallback"));
            continue;
        }
        let Some(key_args) = key_attr(method)? else {
            continue;
        };
        method.attrs.retain(|a| !a.path().is_ident("key"));

        let Some(key) = key_args.key.as_ref() else {
            unreachable!("`key` 参数在解析时已保证存在");
        };
        let key = key_expr(key);
        let desc = key_args.desc.map(|s| s.value()).unwrap_or_default();
        let in_footer = key_args.in_footer;
        let call = if has_receiver {
            quote! { self.#method_name(m) }
        } else {
            let self_ty = &impl_block.self_ty;
            quote! { #self_ty::#method_name(m) }
        };
        bindings.push(quote! {
            Binding {
                mode: #base,
                key: #key,
                desc: #desc,
                in_footer: #in_footer,
            }
        });
        key_arms.push(quote! { #key => { #call } });
    }

    let (draw_ident, draw_has_receiver) = draw.ok_or_else(|| {
        Error::new(
            proc_macro2::Span::call_site(),
            "缺少 #[render] 方法：请标记窗口的绘制方法",
        )
    })?;
    let self_ty = &impl_block.self_ty;
    let draw_call = if draw_has_receiver {
        quote! { self.#draw_ident(m, f) }
    } else {
        quote! { #self_ty::#draw_ident(m, f) }
    };

    let fallback_arm = match &fallback {
        Some((fb, true)) => quote! { _ => self.#fb(m, key), },
        Some((fb, false)) => quote! { _ => #self_ty::#fb(m, key), },
        None => quote! { _ => {} },
    };

    let on_open_impl = if is_popup {
        match &on_open {
            Some((m, true)) => quote! { impl Popup for #self_ty { fn on_open(&mut self, m: &mut Manager) { self.#m(m); } } },
            Some((m, false)) => quote! { impl Popup for #self_ty { fn on_open(&mut self, m: &mut Manager) { #self_ty::#m(m); } } },
            None => quote! { impl Popup for #self_ty {} },
        }
    } else {
        TokenStream2::new()
    };

    Ok(quote! {
        #impl_block

        pub const #base: WindowId = WindowId(#name_str);
        pub static #table_name: LazyLock<Vec<Binding>> = LazyLock::new(|| {
            vec![#(#bindings),*]
        });

        #[allow(clippy::unused_self)]
        impl Window for #self_ty {
            fn handle_key(&mut self, m: &mut Manager, key: KeyCode) {
                match key {
                    #(#key_arms)*
                    #fallback_arm
                }
            }
            fn draw(&mut self, m: &mut Manager, f: &mut Frame) {
                #draw_call;
            }
        }

        #on_open_impl
    })
}

struct WindowsArgs {
    base: String,
    popups: Vec<Ident>,
}

impl Parse for WindowsArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        // 注意：rustc 在过程宏运行前就会按 Meta 解析属性内容，
        // 关键字（如 main）不能作为元数据值，故 base 用字符串字面量
        let ident: Ident = input.parse()?;
        if ident != "base" {
            return Err(Error::new(
                ident.span(),
                "期望 `base = \"<窗口名>\"`，例如 #[windows(base = \"main\", popups = (...))]",
            ));
        }
        input.parse::<Token![=]>()?;
        let base: LitStr = input.parse()?;
        input.parse::<Token![,]>()?;

        let ident: Ident = input.parse()?;
        if ident != "popups" {
            return Err(Error::new(
                ident.span(),
                "期望 `popups = (窗口名, ...)`",
            ));
        }
        input.parse::<Token![=]>()?;
        let content;
        syn::parenthesized!(content in input);
        let mut popups = Vec::new();
        while !content.is_empty() {
            popups.push(content.parse()?);
            if content.is_empty() {
                break;
            }
            content.parse::<Token![,]>()?;
        }
        if !input.is_empty() {
            input.parse::<Token![,]>()?;
        }
        if !input.is_empty() {
            return Err(input.error("多余的参数"));
        }
        Ok(WindowsArgs {
            base: base.value(),
            popups,
        })
    }
}

/// `#[windows(base = main, popups = (url_input, help, ...))]` 属性宏：
/// 挂在占位 item 上（会被消费掉），生成 `Windows` 容器 + 构造 + 按键分发 + 绘制。
#[proc_macro_attribute]
pub fn windows(attr: TokenStream, item: TokenStream) -> TokenStream {
    windows_impl(attr.into(), item.into())
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

fn windows_impl(attr: TokenStream2, item: TokenStream2) -> Result<TokenStream2> {
    let _ = item; // 占位 item 仅消费不解析
    let args: WindowsArgs = parse2(attr)?;

    // 注意：quote! 插值 String 会生成字符串字面量 token，路径必须用 Ident
    let base_name = format_ident!("{}", args.base);
    let base_field = format_ident!("{}", base_name);
    let base_ty = format_ident!("{}Window", to_pascal_case(&base_name.to_string()));
    let base_id = format_ident!("{}", base_name.to_string().to_uppercase());

    let mut fields = Vec::new();
    let mut ctor = Vec::new();
    let mut popup_arms = Vec::new();
    for name in &args.popups {
        let name_str = name.to_string();
        let field = format_ident!("{}", name_str);
        let ty = format_ident!("{}Window", to_pascal_case(&name_str));
        let id = format_ident!("{}", name_str.to_uppercase());
        fields.push(quote! { pub #field: pages::#name::#ty });
        ctor.push(quote! { #field: pages::#name::#ty::new(ctx) });
        popup_arms.push(quote! { pages::#name::#id => Some(&mut self.#field) });
    }

    let output = quote! {
        /// 窗口管理器：持有全部窗口（主窗口 + 弹窗），负责按键分发与绘制。
        pub struct WindowsManager {
            pub #base_field: pages::#base_name::#base_ty,
            #(#fields,)*
        }

        impl WindowsManager {
            pub fn new(ctx: &WindowCtx) -> Self {
                Self {
                    #base_field: pages::#base_name::#base_ty::new(ctx),
                    #(#ctor,)*
                }
            }

            /// 按窗口 ID 取弹窗实例（可变的 trait 对象）
            pub fn popup_mut(&mut self, id: WindowId) -> Option<&mut dyn Popup> {
                match id {
                    #(#popup_arms,)*
                    _ => None,
                }
            }

            /// 按键分发：主窗口常驻；弹窗打开时由当前弹窗处理。
            /// 若按键处理期间切换到了弹窗，触发其 `on_open` 钩子。
            pub fn handle_key(&mut self, m: &mut Manager, key: KeyCode) {
                let prev = m.current_window;
                if prev == pages::#base_name::#base_id {
                    self.#base_field.handle_key(m, key);
                } else if let Some(p) = self.popup_mut(prev) {
                    p.handle_key(m, key);
                }
                if m.current_window != prev && m.current_window != pages::#base_name::#base_id {
                    if let Some(p) = self.popup_mut(m.current_window) {
                        p.on_open(m);
                    }
                }
            }

            /// 绘制：主窗口常驻，弹窗作为覆盖层叠加其上
            pub fn draw(&mut self, m: &mut Manager, f: &mut Frame) {
                self.#base_field.draw(m, f);
                if m.current_window != pages::#base_name::#base_id
                    && let Some(p) = self.popup_mut(m.current_window)
                {
                    p.draw(m, f);
                }
            }
        }
    };

    Ok(output)
}

/// `#[component]` 属性宏：挂在组件 struct 上，生成组件登记项。
///
/// ```ignore
/// #[component]
/// pub struct OperationLog { ... }
/// ```
///
/// 可选标记 `#[component(focusable)]`：可参与 Tab 焦点循环。
/// 展开为原 struct + `pub static COMPONENT_OPERATION_LOG: ComponentEntry`。
/// 要求展开处作用域内有 `ComponentEntry`。
#[proc_macro_attribute]
pub fn component(attr: TokenStream, item: TokenStream) -> TokenStream {
    component_impl(attr.into(), item.into())
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

struct ComponentArgs {
    focusable: bool,
}

impl Parse for ComponentArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut args = ComponentArgs { focusable: false };
        while !input.is_empty() {
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
                continue;
            }
            let ident: Ident = input.parse()?;
            match ident.to_string().as_str() {
                "focusable" => args.focusable = true,
                _ => {
                    return Err(Error::new(
                        ident.span(),
                        format!("未知参数 `{ident}`，可用：focusable"),
                    ));
                }
            }
            if input.is_empty() {
                break;
            }
            input.parse::<Token![,]>()?;
        }
        Ok(args)
    }
}

fn component_impl(attr: TokenStream2, item: TokenStream2) -> Result<TokenStream2> {
    let args: ComponentArgs = if attr.is_empty() {
        ComponentArgs { focusable: false }
    } else {
        parse2(attr)?
    };
    let item_struct: ItemStruct = parse2(item)?;
    let struct_ident = &item_struct.ident;
    let name = to_snake_case(struct_ident);
    let const_name = format_ident!("COMPONENT_{}", name.to_uppercase());
    let focusable = args.focusable;
    Ok(quote! {
        #item_struct

        pub static #const_name: ComponentEntry = ComponentEntry {
            name: #name,
            focusable: #focusable,
        };
    })
}

/// `OperationLog` -> "operation_log"（仅处理纯驼峰，够本项目用）
fn to_snake_case(ident: &Ident) -> String {
    let s = ident.to_string();
    let mut out = String::with_capacity(s.len() + 4);
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

/// `url_input` -> `UrlInput`
fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// 找出方法上的 `#[key(...)]` 属性并解析其参数
fn key_attr(method: &syn::ImplItemFn) -> Result<Option<KeyArgs>> {
    for attr in &method.attrs {
        if attr.path().is_ident("key") {
            return match &attr.meta {
                Meta::List(list) => Ok(Some(parse2(list.tokens.clone())?)),
                _ => Err(Error::new(
                    attr.span(),
                    "期望 `#[key(...)]` 形式，例如 #[key('q', desc = \"退出\")]",
                )),
            };
        }
    }
    Ok(None)
}

/// 字符字面量 `'q'` 自动包装为 `KeyCode::Char('q')`，其余原样透传
fn key_expr(expr: &Expr) -> TokenStream2 {
    if let Expr::Lit(lit) = expr
        && let Lit::Char(c) = &lit.lit
    {
        return quote! { KeyCode::Char(#c) };
    }
    quote! { #expr }
}

#[derive(Default)]
struct KeyArgs {
    key: Option<Expr>,
    desc: Option<LitStr>,
    in_footer: bool,
}

impl Parse for KeyArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut args = KeyArgs::default();
        loop {
            if input.is_empty() {
                break;
            }
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
                continue;
            }
            if input.peek(Ident) {
                let ident: Ident = input.fork().parse()?;
                match ident.to_string().as_str() {
                    "desc" => {
                        input.parse::<Ident>()?;
                        input.parse::<Token![=]>()?;
                        let value: Expr = input.parse()?;
                        let Expr::Lit(lit) = value else {
                            return Err(Error::new(value.span(), "`desc` 必须是字符串字面量"));
                        };
                        let Lit::Str(s) = lit.lit else {
                            return Err(Error::new(lit.span(), "`desc` 必须是字符串字面量"));
                        };
                        args.desc = Some(s);
                    }
                    "footer" => {
                        input.parse::<Ident>()?;
                        args.in_footer = true;
                    }
                    _ if args.key.is_none() => {
                        args.key = Some(input.parse()?);
                    }
                    _ => {
                        return Err(Error::new(
                            ident.span(),
                            format!("未知参数 `{ident}`，可用：desc / footer"),
                        ));
                    }
                }
            } else if args.key.is_none() {
                args.key = Some(input.parse()?);
            } else {
                return Err(input.error("多余的参数"));
            }
            if input.is_empty() {
                break;
            }
            input.parse::<Token![,]>()?;
        }
        if args.key.is_none() {
            return Err(Error::new(
                proc_macro2::Span::call_site(),
                "缺少按键参数，例如 #[key('q', desc = \"退出\")]",
            ));
        }
        Ok(args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_str;

    #[test]
    fn test_windows_args_parse() {
        let args: WindowsArgs = parse_str(r#"base = "main", popups = (url_input, provider_select, help, mihomo_log)"#).unwrap();
        assert_eq!(args.base, "main");
        assert_eq!(args.popups.len(), 4);
    }
}
