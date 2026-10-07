fn main() {
    build_speex();
    tauri_build::build()
}

/// 编译随仓库分发的 libspeex 1.2.1（BSD-3，见 third_party/speex/COPYING），
/// 用于把词典里的 .spx 发音转成 WebView2 能播放的 WAV。
/// 宏定义与 speex-sys 一致；不用 bindgen，FFI 声明手写在 src/audio.rs。
fn build_speex() {
    const FILES: &[&str] = &[
        "bits", "cb_search", "exc_5_64_table", "exc_5_256_table", "exc_8_128_table", "exc_10_16_table",
        "exc_10_32_table", "exc_20_32_table", "filters", "gain_table", "gain_table_lbr", "hexc_10_32_table",
        "hexc_table", "high_lsp_tables", "kiss_fft", "kiss_fftr", "lpc", "lsp", "lsp_tables_nb", "ltp", "modes",
        "modes_wb", "nb_celp", "quant_lsp", "sb_celp", "smallft", "speex", "speex_callbacks", "speex_header",
        "stereo", "vbr", "vorbis_psy", "vq", "window",
    ];
    let mut build = cc::Build::new();
    build.include("third_party/speex/include").include("third_party/speex/libspeex");
    for f in FILES {
        build.file(format!("third_party/speex/libspeex/{f}.c"));
    }
    build.define("FLOATING_POINT", None).define("EXPORT", Some("")).warnings(false);
    build.compile("speex");
    println!("cargo:rerun-if-changed=third_party/speex");
}
