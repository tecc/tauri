// Copyright 2019-2024 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

use proc_macro2::TokenStream;
use quote::{format_ident, ToTokens};
use syn::{parse::{Parse, ParseBuffer, ParseStream}, Attribute, Expr, Ident, Lit, LitBool, LitStr, Meta, Path, Token};
use syn::spanned::Spanned;

struct CommandDef {
  path: Path,
  attrs: Vec<Attribute>,
}

impl Parse for CommandDef {
  fn parse(input: ParseStream) -> syn::Result<Self> {
    let attrs = input.call(Attribute::parse_outer)?;
    let path = input.parse()?;

    Ok(CommandDef { path, attrs })
  }
}

struct HandlerOptions {
  /// This preserves the old naming system with the last ident in a path becoming the command name.
  ///
  /// Setting this to false will use the new system that allows renaming from the
  /// [`command`](crate::command) attribute macro.
  use_new_name: bool
}
impl Parse for HandlerOptions {
  fn parse(input: ParseStream) -> syn::Result<Self> {
    let options = input.call(Attribute::parse_inner)?;
    let mut parsed = HandlerOptions {
      use_new_name: false
    };

    let mut error: syn::Result<()> = Ok(());
    for option in options {
      match option.meta {
        Meta::NameValue(v) => {
          if v.path.is_ident("use_new_name") {
            if let Expr::Lit(syn::ExprLit { lit: Lit::Bool(bool), .. }) = v.value {
              parsed.use_new_name = bool.value
            }
          }
        }
        _ => {
          // This allows multiple option errors to surface at once, for convenience
          let err = syn::Error::new_spanned(option.meta, "unrecognised option");
          if let Err(error) = &mut error {
            error.combine(err)
          } else {
            error = Err(err)
          }
        }
      }
    }

    error?;
    Ok(parsed)
  }
}

/// The items parsed from [`generate_handler!`](crate::generate_handler).
pub struct Handler {
  options: HandlerOptions,
  command_defs: Vec<CommandDef>,
  commands: Vec<TokenStream>,
  wrappers: Vec<Path>,
}

impl Parse for Handler {
  fn parse(input: &ParseBuffer<'_>) -> syn::Result<Self> {
    let options: HandlerOptions = input.parse()?;
    let command_defs = input.parse_terminated(CommandDef::parse, Token![,])?;

    // parse the command names and wrappers from the passed paths
    let (commands, wrappers) = command_defs
      .iter()
      .map(|command_def| {
        let mut wrapper = command_def.path.clone();

        let wrapper_last = super::path_to_command(&mut wrapper);

        // the name of the actual command function
        let command_name = wrapper_last.ident.clone();

        let mut info = command_def.path.clone();
        let info_last = super::path_to_command(&mut info);
        info_last.ident = super::format_command_info(&command_name);

        // the name that is invoked, or rather, "handled"
        let command = if options.use_new_name {
          quote::quote!(__tauri_cmd__ if __tauri_cmd__ == #info.name)
        } else {
          quote::quote!(stringify!(#command_name))
        };

        // set the path to the command function wrapper
        wrapper_last.ident = super::format_command_wrapper(&command_name);


        (command, wrapper)
      })
      .unzip();

    Ok(Self {
      options,
      command_defs: command_defs.into_iter().collect(), // remove punctuation separators
      commands,
      wrappers,
    })
  }
}

impl From<Handler> for proc_macro::TokenStream {
  fn from(
    Handler {
      options: _,
      command_defs,
      commands,
      wrappers,
    }: Handler,
  ) -> Self {
    let cmd = format_ident!("__tauri_cmd__");
    let invoke = format_ident!("__tauri_invoke__");
    let (paths, attrs): (Vec<Path>, Vec<Vec<Attribute>>) = command_defs
      .into_iter()
      .map(|def| (def.path, def.attrs))
      .unzip();
    quote::quote!(move |#invoke| {
      let #cmd = #invoke.message.command();
      match #cmd {
        #(#(#attrs)* #commands => #wrappers!(#paths, #invoke),)*
        _ => {
          return false;
        },
      }
    })
    .into()
  }
}
