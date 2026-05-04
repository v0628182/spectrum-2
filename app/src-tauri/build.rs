fn main() {
    build_warzone_audio_core();
    tauri_build::build()
}

fn build_warzone_audio_core() {
    let root = "native/warzone_audio";
    let mut build = cc::Build::new();

    build
        .cpp(true)
        .std("c++17")
        .include(format!("{root}/include"))
        .include(format!("{root}/src"))
        .file(format!("{root}/src/CApi.cpp"))
        .file(format!("{root}/src/Biquad.cpp"))
        .file(format!("{root}/src/DspEngine.cpp"))
        .file(format!("{root}/src/Fft.cpp"))
        .file(format!("{root}/src/FeatureExtractor.cpp"))
        .file(format!("{root}/src/Processor.cpp"))
        .file(format!("{root}/src/RealtimeEngine.cpp"))
        .file(format!("{root}/src/RealtimeCApi.cpp"))
        .file(format!("{root}/src/SelfWeaponSuppressor.cpp"))
        .file(format!("{root}/src/SpatialDspEngine.cpp"))
        .file(format!("{root}/src/TransientDetector.cpp"));

    if build.get_compiler().is_like_msvc() {
        build.flag("/EHsc").flag("/permissive-");
    }

    build.compile("warzone_audio_core");
}
