//! Each invocation gets a fresh WASI instance, preserving upstream utility startup.

#[cfg(target_os = "wasi")]
#[link(wasm_import_module = "parsh")]
unsafe extern "C" {
    fn cwd_len() -> usize;
    fn cwd_write(destination: *mut u8);
}

fn working_directory() -> std::io::Result<()> {
    #[cfg(target_os = "wasi")]
    {
        // The host writes exactly cwd_len UTF-8 bytes into this guest allocation.
        let mut bytes = vec![0; unsafe { cwd_len() }];
        unsafe { cwd_write(bytes.as_mut_ptr()) };
        let cwd = String::from_utf8(bytes).map_err(std::io::Error::other)?;
        std::env::set_current_dir(cwd)?;
    }
    Ok(())
}

mod command_echo {
    uucore::bin!(uu_echo);
}
mod command_printf {
    uucore::bin!(uu_printf);
}
mod command_pwd {
    uucore::bin!(uu_pwd);
}
mod command_ls {
    uucore::bin!(uu_ls);
}
mod command_cat {
    uucore::bin!(uu_cat);
}
mod command_head {
    uucore::bin!(uu_head);
}
mod command_tail {
    uucore::bin!(uu_tail);
}
mod command_sort {
    uucore::bin!(uu_sort);
}
mod command_uniq {
    uucore::bin!(uu_uniq);
}
mod command_wc {
    uucore::bin!(uu_wc);
}
mod command_seq {
    uucore::bin!(uu_seq);
}
mod command_cut {
    uucore::bin!(uu_cut);
}
mod command_tr {
    uucore::bin!(uu_tr);
}
mod command_tee {
    uucore::bin!(uu_tee);
}
mod command_nl {
    uucore::bin!(uu_nl);
}
mod command_base64 {
    uucore::bin!(uu_base64);
}
mod command_sha256sum {
    uucore::bin!(uu_sha256sum);
}
mod command_basename {
    uucore::bin!(uu_basename);
}
mod command_dirname {
    uucore::bin!(uu_dirname);
}
mod command_mkdir {
    uucore::bin!(uu_mkdir);
}
mod command_rmdir {
    uucore::bin!(uu_rmdir);
}
mod command_touch {
    uucore::bin!(uu_touch);
}
mod command_cp {
    uucore::bin!(uu_cp);
}
mod command_mv {
    uucore::bin!(uu_mv);
}
mod command_rm {
    uucore::bin!(uu_rm);
}
mod command_test {
    uucore::bin!(uu_test);
}
mod command_true {
    uucore::bin!(uu_true);
}
mod command_false {
    uucore::bin!(uu_false);
}
mod command_printenv {
    uucore::bin!(uu_printenv);
}

fn main() {
    if let Err(error) = working_directory() {
        eprintln!("coreutils: working directory: {error}");
        std::process::exit(2);
    }
    let command = std::env::args_os().next().unwrap_or_default();
    match command.to_str() {
        Some("echo") => command_echo::main(),
        Some("printf") => command_printf::main(),
        Some("pwd") => command_pwd::main(),
        Some("ls") => command_ls::main(),
        Some("cat") => command_cat::main(),
        Some("head") => command_head::main(),
        Some("tail") => command_tail::main(),
        Some("sort") => command_sort::main(),
        Some("uniq") => command_uniq::main(),
        Some("wc") => command_wc::main(),
        Some("seq") => command_seq::main(),
        Some("cut") => command_cut::main(),
        Some("tr") => command_tr::main(),
        Some("tee") => command_tee::main(),
        Some("nl") => command_nl::main(),
        Some("base64") => command_base64::main(),
        Some("sha256sum") => command_sha256sum::main(),
        Some("basename") => command_basename::main(),
        Some("dirname") => command_dirname::main(),
        Some("mkdir") => command_mkdir::main(),
        Some("rmdir") => command_rmdir::main(),
        Some("touch") => command_touch::main(),
        Some("cp") => command_cp::main(),
        Some("mv") => command_mv::main(),
        Some("rm") => command_rm::main(),
        Some("test") => command_test::main(),
        Some("true") => command_true::main(),
        Some("false") => command_false::main(),
        Some("printenv") => command_printenv::main(),
        _ => {
            eprintln!("coreutils: unknown command {}", command.to_string_lossy());
            std::process::exit(127);
        }
    }
}
