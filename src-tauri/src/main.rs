// Sem console no Windows quando roda o executável instalado.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    ponte_lib::rodar()
}
