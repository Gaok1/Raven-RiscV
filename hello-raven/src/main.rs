#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;

mod raven_api;

use alloc::vec;
use alloc::vec::Vec;

use crate::raven_api::syscall::sys_exit;

// ── Entry point ───────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    main()
}

// ── Pequeno utilitário para evitar otimizações agressivas ────────────────────

#[inline(never)]
fn black_box_u64(x: u64) -> u64 {
    core::hint::black_box(x)
}

// ── Experimentos ──────────────────────────────────────────────────────────────

// 1) Acesso sequencial: melhor caso para cache + prefetcher
#[inline(never)]
fn sequential_sum(data: &[u64]) -> u64 {
    let mut acc = 0u64;
    let mut i = 0usize;

    while i < data.len() {
        acc = acc.wrapping_add(data[i]);
        i += 1;
    }

    black_box_u64(acc)
}

// 2) Acesso com stride: pula elementos.
// Quanto maior o stride, menos localidade espacial você aproveita.
#[inline(never)]
fn strided_sum(data: &[u64], stride: usize) -> u64 {
    let mut acc = 0u64;
    let mut i = 0usize;

    while i < data.len() {
        acc = acc.wrapping_add(data[i]);
        i += stride;
    }

    black_box_u64(acc)
}

// 3) Matriz em row-major.
// Percorrer por linhas respeita o layout em memória.
#[inline(never)]
fn row_major_sum(matrix: &[u64], rows: usize, cols: usize) -> u64 {
    let mut acc = 0u64;
    let mut r = 0usize;

    while r < rows {
        let base = r * cols;
        let mut c = 0usize;
        while c < cols {
            acc = acc.wrapping_add(matrix[base + c]);
            c += 1;
        }
        r += 1;
    }

    black_box_u64(acc)
}

// 4) Mesmo dado, mas acesso por colunas.
// Em layout row-major, isso tende a ser pior.
#[inline(never)]
fn col_major_sum(matrix: &[u64], rows: usize, cols: usize) -> u64 {
    let mut acc = 0u64;
    let mut c = 0usize;

    while c < cols {
        let mut r = 0usize;
        while r < rows {
            acc = acc.wrapping_add(matrix[r * cols + c]);
            r += 1;
        }
        c += 1;
    }

    black_box_u64(acc)
}

// 5) Pointer chasing artificial.
// Cada posição aponta para a próxima. Isso reduz previsibilidade e
// atrapalha bastante prefetch e paralelismo de memória.
#[inline(never)]
fn pointer_chase(next: &[usize], steps: usize) -> usize {
    let mut idx = 0usize;
    let mut s = 0usize;

    while s < steps {
        idx = next[idx];
        s += 1;
    }

    core::hint::black_box(idx)
}

// 6) Escrever em array pequeno repetidamente.
// Working set pequeno: tende a caber em cache.
#[inline(never)]
fn hot_write(data: &mut [u64], rounds: usize) -> u64 {
    let mut round = 0usize;

    while round < rounds {
        let mut i = 0usize;
        while i < data.len() {
            data[i] = data[i].wrapping_add(1);
            i += 1;
        }
        round += 1;
    }

    sequential_sum(data)
}

// 7) Escrever em array grande.
// Mesmo padrão, mas com working set maior.
#[inline(never)]
fn cold_write(data: &mut [u64], rounds: usize) -> u64 {
    let mut round = 0usize;

    while round < rounds {
        let mut i = 0usize;
        while i < data.len() {
            data[i] = data[i].wrapping_add(1);
            i += 1;
        }
        round += 1;
    }

    sequential_sum(data)
}

// ── Geração de dados ──────────────────────────────────────────────────────────

fn make_linear_data(n: usize) -> Vec<u64> {
    let mut v = Vec::with_capacity(n);
    let mut i = 0usize;

    while i < n {
        v.push((i as u64).wrapping_mul(13).wrapping_add(7));
        i += 1;
    }

    v
}

// Gera uma permutação cíclica simples para pointer chasing.
// next[i] = (i + step) % n
fn make_chase_ring(n: usize, step: usize) -> Vec<usize> {
    let mut v = vec![0usize; n];
    let mut i = 0usize;

    while i < n {
        v[i] = (i + step) % n;
        i += 1;
    }

    v
}

// ── Programa principal ────────────────────────────────────────────────────────

fn main() -> ! {
    // Tamanhos escolhidos só para demonstrar padrões.
    // Ajuste depois para estressar L1/L2/L3 dependendo do ambiente.
    let small_n = 4 * 1024;          // 4k u64  = 32 KiB
    let medium_n = 64 * 1024;        // 64k u64 = 512 KiB
    let large_n = 512 * 1024;        // 512k u64 = 4 MiB

    let small = make_linear_data(small_n);
    let medium = make_linear_data(medium_n);
    let large = make_linear_data(large_n);

    println!("================ CACHE EXPERIMENTS ================");
    println!();

    println!("1) Sequential vs Strided");
    println!("---------------------------------------------------");
    println!("small sequential   = {}", sequential_sum(&small));
    println!("small stride 2     = {}", strided_sum(&small, 2));
    println!("small stride 8     = {}", strided_sum(&small, 8));
    println!("small stride 64    = {}", strided_sum(&small, 64));
    println!();

    println!("medium sequential  = {}", sequential_sum(&medium));
    println!("medium stride 2    = {}", strided_sum(&medium, 2));
    println!("medium stride 8    = {}", strided_sum(&medium, 8));
    println!("medium stride 64   = {}", strided_sum(&medium, 64));
    println!();

    println!("large sequential   = {}", sequential_sum(&large));
    println!("large stride 2     = {}", strided_sum(&large, 2));
    println!("large stride 8     = {}", strided_sum(&large, 8));
    println!("large stride 64    = {}", strided_sum(&large, 64));
    println!();

    // Matriz 256 x 256 => 65536 elementos
    let rows = 256usize;
    let cols = 256usize;
    let matrix = make_linear_data(rows * cols);

    println!("2) Row-major vs Column-major");
    println!("---------------------------------------------------");
    println!("row-major sum      = {}", row_major_sum(&matrix, rows, cols));
    println!("col-major sum      = {}", col_major_sum(&matrix, rows, cols));
    println!();

    let chase_small = make_chase_ring(8 * 1024, 97);
    let chase_large = make_chase_ring(512 * 1024, 97);

    println!("3) Pointer chasing");
    println!("---------------------------------------------------");
    println!("chase small        = {}", pointer_chase(&chase_small, 100_000));
    println!("chase large        = {}", pointer_chase(&chase_large, 100_000));
    println!();

    let mut hot = make_linear_data(4 * 1024);        // ~32 KiB
    let mut cold = make_linear_data(512 * 1024);     // ~4 MiB

    println!("4) Hot working set vs Cold working set");
    println!("---------------------------------------------------");
    println!("hot write          = {}", hot_write(&mut hot, 16));
    println!("cold write         = {}", cold_write(&mut cold, 16));
    println!();
    println!("===================================================");

    sys_exit(0)
}