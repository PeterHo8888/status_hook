#![feature(proc_macro_hygiene)]
#![allow(unused_unsafe)]

use skyline::{hook, install_hook};
use skyline::nro::{self, NroInfo};

use smash::*;

use lazy_static::lazy_static;
use std::collections::HashMap;
use std::sync::Mutex;

type StatusFunc = unsafe fn(l2c_fighter: *mut lua2cpp::L2CFighterCommon, l2c_agent: u64) -> lib::L2CValue;

static mut LOG_LEVEL: i32 = 1;

macro_rules! debugln {
    ($($args:expr),*) => {
        unsafe {
            match LOG_LEVEL {
                1 => println!($($args),*),
                _ => (),
            }
        }
    }
}

#[derive(Debug)]
struct Nro {
    start: u64,
    end: u64
}

struct StatusInfo {
    status_kind: i32,
    lua_script: i32,
    func: StatusFunc,
    orig: u64
}
extern "C" {
    #[link_name = "\u{1}_ZN7lua2cpp12L2CAgentBase18sv_set_status_funcERKN3lib8L2CValueES4_Pv"]
    fn status_internal_sv_set_status_func(this: &lua2cpp::L2CAgentBase, status_kind: &lib::L2CValue, lua_script: &lib::L2CValue, func: u64);
}

lazy_static! {
    static ref NRO_MAP: Mutex<HashMap<String, Nro>> = Mutex::new(HashMap::new());
    static ref FUNC_MAP: Mutex<HashMap<String, Vec<StatusInfo>>> = Mutex::new(HashMap::new());
}

fn stub(_l2c_fighter: *mut lua2cpp::L2CFighterCommon, _l2c_agent: u64) -> lib::L2CValue {
    println!("[status_kind] ERROR: Original func was not set!");
    lib::L2CValue::new_int(0)
}

#[hook(replace = status_internal_sv_set_status_func)]
unsafe fn status_replace_sv_set_status_func(this: u64, status_kind: &lib::L2CValue, lua_script: &lib::L2CValue, func: u64) {
    let map = NRO_MAP.lock().unwrap();

    for (fighter, nro) in map.iter() {
        if func > nro.start && func < nro.end {
            let mut funcs = FUNC_MAP.lock().unwrap();

            let this_status = status_kind.get_int() as i32;
            let this_lua = lua_script.get_int() as i32;

            if let Some(fighter_funcs) = funcs.get_mut(fighter) {
                for status in fighter_funcs.iter_mut() {
                    if status.status_kind == this_status && status.lua_script == this_lua {
                        //debugln!("[status_hook] Replacing status func for {}: {} - {}", fighter, fighter_status_str(this_status), lua_script_str(this_lua));
                        debugln!("[status_hook] Replacing status func for {}: {} - {}", fighter, this_status, this_lua);
                        status.orig = func;
                        original!()(this, status_kind, lua_script, status.func as u64);
                        return;
                    }
                }
            }
            break;
        }
    }
    original!()(this, status_kind, lua_script, func)
}

fn nro_main(nro: &NroInfo) {
    match nro.name {
        "common" => {
            unsafe {
                let text = (*(nro.module.ModuleObject)).module_base;
                debugln!("[status_hook] Common: 0x{:x}", text);
            }
            install_hook!(status_replace_sv_set_status_func);
            debugln!("[status_hook] Installed status_replace_sv_set_status_func");
        }
        name => {
            if NRO_MAP.lock().unwrap().contains_key(name) {
                //debugln!("[status_hook] Updating NRO base for {}", name);
            }
            unsafe {
                let module_obj = *nro.module.ModuleObject;
                let base = module_obj.module_base;
                //let end = base + module_obj.nro_size;
                let end = base + 0xfffffff;  // Can't seem to figure out nro_size, but this value is safe

                let nro = Nro{start: base, end: end};
                debugln!("[status_hook] {}: 0x{:x} - 0x{:x}", name, base, end);
                NRO_MAP.lock().unwrap().insert(String::from(name), nro);
            }
        }
    }
}

#[no_mangle]
pub extern "Rust" fn replace_status_func(fighter_str: &'static str, status_kind: i32, lua_script: i32, func: StatusFunc) {
    let s = StatusInfo{status_kind: status_kind, lua_script: lua_script, func: func, orig: stub as u64};

    let mut funcs = FUNC_MAP.lock().unwrap();
    if let Some(x) = funcs.get_mut(fighter_str) {
        x.push(s);
    } else {
        funcs.insert(String::from(fighter_str), vec![s]);
    }
    debugln!("[status_hook] Added func for {}", fighter_str);
}

#[no_mangle]
pub extern "Rust" fn call_original(
    fighter_str: &'static str,
    status_kind: i32,
    lua_script: i32,
    l2c_fighter: *mut lua2cpp::L2CFighterCommon,
    l2c_agent: u64
) -> lib::L2CValue {
    let funcs = FUNC_MAP.lock().unwrap();
    if let Some(x) = funcs.get(fighter_str) {
        for f in x {
            if f.status_kind == status_kind && f.lua_script == lua_script {
                unsafe {
                    let orig: StatusFunc = std::mem::transmute(f.orig as *const StatusFunc);
                    return orig(l2c_fighter, l2c_agent)
                }
            }
        }
    } else {
        println!("[status_hook] ERROR: Could not find original function!");
    }
    lib::L2CValue::new_int(0)
}


#[skyline::main(name = "status_hook")]
pub fn main() {
    println!("[status_hook] Hello from status_hook!");
    nro::add_hook(nro_main).unwrap();
}
