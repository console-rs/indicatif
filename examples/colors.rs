extern crate indicatif;

use indicatif::style;

fn main() {
    println!("This is {:010x}", style(42).red().on_black().bold());
    println!("This is invisible: [{}]", style("whatever").hidden());
    println!("This is cyan: {}", style("whatever").cyan());
}
