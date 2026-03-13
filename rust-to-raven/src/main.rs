#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;

mod raven_api;

use alloc::alloc::Layout;
use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;

use crate::raven_api::syscall::{sys_exit, sys_getrandom, sys_pause_sim};

fn random_i32_bounded(limit: i32) -> i32 {

    let mut bytes = [0u8; 4];

    let ret = unsafe { sys_getrandom(bytes.as_mut_ptr(), 4, 0) };

    if ret < 0 {
        eprintln!("falha em sys_getrandom: {}", ret);
        sys_exit(3);
    }

    i32::from_ne_bytes(bytes) % limit

}

fn fill_random_i32(values: &mut [i32], limit: i32) {
    for value in values.iter_mut() {
        *value = random_i32_bounded(limit);
    }
}

fn btree_sort(values: &[i32]) -> Vec<i32> {
    let mut freq = BTreeMap::<i32, usize>::new();

    for &value in values {
        match freq.get_mut(&value) {
            Some(count) => *count += 1,
            None => {
                freq.insert(value, 1);
            }
        }
    }

    let mut out = Vec::with_capacity(values.len());

    for (value, count) in freq.iter() {
        for _ in 0..*count {
            out.push(*value);
        }
    }

    out
}

fn print_array(label: &str, values: &[i32]) {
    print!("{} [", label);

    for (i, value) in values.iter().enumerate() {
        if i > 0 {
            print!(", ");
        }
        print!("{}", value);
    }

    println!("]");
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    println!("Exemplo: ordenacao com BTreeMap");

    let mut values = vec![0i32; 20];

    fill_random_i32(&mut values, 100);

    print_array("Array original:", &values);

    let sorted = btree_sort(&values);

    print_array("Array ordenado:", &sorted);

    println!("Fim da execucao.");
    sys_pause_sim();
    sys_exit(0);
}

