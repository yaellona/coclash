//! 过程宏：以 C# 特性（Attribute）的方式注册按键绑定。
//!
//! 用法：在 `impl App` 块上挂 `#[keymap]`，块内方法挂 `#[key(...)]`，
//! 宏会收集所有标记的方法，在展开处自动生成静态注册表：
//!
//! ```ignore
//! #[keymap]
//! impl App {
//!     #[key('q', desc = "退出", footer)]
//!     fn key_quit(&mut self) { self.should_quit = true; }
//! }
//! ```
//!
//! 支持 `#[keymap(name = "NAME")]` 指定注册表名（默认 `BINDINGS`），
//! 方便把不同页面/模式的按键分散到各自模块中定义，再统一聚合。
//! 同一作用域内注册表名不能重复（重复会产生编译错误）。
//!
//! `#[key(...)]` 参数：
//! - 第一个位置参数为按键：字符字面量 `'q'` 自动转为 `KeyCode::Char('q')`，
//!   其他写法原样作为表达式（如 `KeyCode::Up`）
//! - `mode = <expr>`：生效的弹窗模式，默认 `PopupMode::None`
//! - `desc = "..."`：帮助文案，默认空字符串（不展示）
//! - `footer`：标记位，出现在底部栏
//!
//! 生成代码要求展开处作用域内有 `Binding` 结构（字段
//! `mode/key/desc/in_footer/run`，`run: fn(&mut App)`）和 `LazyLock`。

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    parse::{Parse, ParseStream},
    parse2, parse_quote, spanned::Spanned, Error, Expr, Ident, ImplItem, ItemImpl, Lit, LitStr,
    Meta, Result, Token,
};

/// `#[keymap]` 属性宏：展开为原 impl 块 + 生成的静态注册表。
/// 可选参数 `name = "XXX"` 指定注册表名，默认 `BINDINGS`。
#[proc_macro_attribute]
pub fn keymap(attr: TokenStream, item: TokenStream) -> TokenStream {
    keymap_impl(attr.into(), item.into())
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

struct KeymapArgs {
    name: Option<LitStr>,
}

impl Parse for KeymapArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.is_empty() {
            return Ok(KeymapArgs { name: None });
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
        Ok(KeymapArgs { name: Some(name) })
    }
}

fn keymap_impl(attr: TokenStream2, item: TokenStream2) -> Result<TokenStream2> {
    let args: KeymapArgs = if attr.is_empty() {
        KeymapArgs { name: None }
    } else {
        parse2(attr)?
    };
    let static_name = match &args.name {
        Some(name) => format_ident!("{}", name.value()),
        None => format_ident!("BINDINGS"),
    };
    let mut impl_block: ItemImpl = parse2(item)?;
    if impl_block.trait_.is_some() {
        return Err(Error::new_spanned(
            &impl_block,
            "#[keymap] 只能用在固有 impl 块上",
        ));
    }

    let mut bindings = Vec::new();
    for item in &mut impl_block.items {
        let ImplItem::Fn(method) = item else {
            continue;
        };
        let Some(key_args) = key_attr(method)? else {
            continue;
        };
        method.attrs.retain(|a| !a.path().is_ident("key"));

        let Some(key) = key_args.key.as_ref() else {
            unreachable!("`key` 参数在解析时已保证存在");
        };
        let key = key_expr(key);
        let mode = key_args.mode.unwrap_or_else(|| parse_quote!(PopupMode::None));
        let desc = key_args.desc.map(|s| s.value()).unwrap_or_default();
        let in_footer = key_args.in_footer;
        let method_name = &method.sig.ident;
        let self_ty = &impl_block.self_ty;
        bindings.push(quote! {
            Binding {
                mode: #mode,
                key: #key,
                desc: #desc,
                in_footer: #in_footer,
                run: #self_ty::#method_name,
            }
        });
    }

    Ok(quote! {
        #impl_block

        pub static #static_name: LazyLock<Vec<Binding>> = LazyLock::new(|| {
            vec![#(#bindings),*]
        });
    })
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
    mode: Option<Expr>,
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
                    "mode" | "desc" => {
                        input.parse::<Ident>()?;
                        input.parse::<Token![=]>()?;
                        let value: Expr = input.parse()?;
                        if ident == "mode" {
                            if args.mode.replace(value).is_some() {
                                return Err(Error::new(ident.span(), "`mode` 重复指定"));
                            }
                        } else {
                            let Expr::Lit(lit) = value else {
                                return Err(Error::new(value.span(), "`desc` 必须是字符串字面量"));
                            };
                            let Lit::Str(s) = lit.lit else {
                                return Err(Error::new(lit.span(), "`desc` 必须是字符串字面量"));
                            };
                            args.desc = Some(s);
                        }
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
                            format!("未知参数 `{ident}`，可用：mode / desc / footer"),
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
