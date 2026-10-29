use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

#[allow(unreachable_pub)]
mod sha512_constants {
    include!("src/sha512_constants.rs");
}

const NATIVE_TEMPLATE: &str = "src/shaders/sha512_search.wgsl";
const WEB_TEMPLATE: &str = "src/shaders/sha512_web.wgsl";
const CUDA_TEMPLATE: &str = "src/shaders/sha512_search.cu";
const ROUND_MARKER: &str = "    // @SHA512_UNROLLED_ROUNDS@";

#[derive(Clone, Copy)]
enum Shader {
    Native,
    Web,
    Cuda,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for path in [
        NATIVE_TEMPLATE,
        WEB_TEMPLATE,
        CUDA_TEMPLATE,
        "src/sha512_constants.rs",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
    println!("cargo:rerun-if-changed=build.rs");

    let output_dir =
        PathBuf::from(std::env::var_os("OUT_DIR").ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "OUT_DIR is not set")
        })?);
    generate_shader(
        NATIVE_TEMPLATE,
        output_dir.join("sha512_search.wgsl"),
        rounds(Shader::Native)?,
    )?;
    generate_shader(
        WEB_TEMPLATE,
        output_dir.join("sha512_web.wgsl"),
        rounds(Shader::Web)?,
    )?;
    generate_shader(
        CUDA_TEMPLATE,
        output_dir.join("sha512_search.cu"),
        rounds(Shader::Cuda)?,
    )?;

    Ok(())
}

fn generate_shader(
    template_path: &str,
    output_path: PathBuf,
    rounds: String,
) -> Result<(), std::io::Error> {
    let template = fs::read_to_string(template_path)?;
    if template.matches(ROUND_MARKER).count() != 1 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("shader template {template_path} must contain one round marker"),
        ));
    }
    fs::write(output_path, template.replace(ROUND_MARKER, &rounds))
}

fn rounds(shader: Shader) -> Result<String, std::fmt::Error> {
    let mut output = String::new();
    let mut state = ["a", "b", "c", "d", "e", "f", "g", "h"];

    for (round, constant) in sha512_constants::ROUND_CONSTANTS.iter().enumerate() {
        let slot = round & 15;
        if round >= 16 {
            match shader {
                Shader::Native | Shader::Cuda => writeln!(
                    output,
                    "    w{slot} = small_sigma1(w{}) + w{} + small_sigma0(w{}) + w{slot};",
                    (round + 14) & 15,
                    (round + 9) & 15,
                    (round + 1) & 15
                )?,
                Shader::Web => writeln!(
                    output,
                    "    w{slot} = add64(add64(small_sigma1(w{}), w{}), add64(small_sigma0(w{}), w{slot}));",
                    (round + 14) & 15,
                    (round + 9) & 15,
                    (round + 1) & 15
                )?,
            }
        }
        let [a, b, c, d, e, f, g, h] = state;
        let high = constant >> 32;
        let low = constant & 0xffff_ffff;
        match shader {
            Shader::Native => {
                writeln!(
                    output,
                    "    let temp1_{round} = {h} + big_sigma1({e}) + (({e} & {f}) ^ ((~{e}) & {g})) + 0x{constant:016x}lu + w{slot};"
                )?;
                writeln!(
                    output,
                    "    let temp2_{round} = big_sigma0({a}) + (({a} & {b}) ^ ({a} & {c}) ^ ({b} & {c}));"
                )?;
                writeln!(output, "    {d} = {d} + temp1_{round};")?;
                writeln!(output, "    {h} = temp1_{round} + temp2_{round};")?;
            }
            Shader::Web => {
                writeln!(
                    output,
                    "    let temp1_{round} = add64(add64(add64({h}, big_sigma1({e})), choice({e}, {f}, {g})), add64(vec2<u32>(0x{high:08x}u, 0x{low:08x}u), w{slot}));"
                )?;
                writeln!(
                    output,
                    "    let temp2_{round} = add64(big_sigma0({a}), majority({a}, {b}, {c}));"
                )?;
                writeln!(output, "    {d} = add64({d}, temp1_{round});")?;
                writeln!(output, "    {h} = add64(temp1_{round}, temp2_{round});")?;
            }
            Shader::Cuda => {
                writeln!(
                    output,
                    "    unsigned long long temp1_{round} = {h} + big_sigma1({e}) + (({e} & {f}) ^ ((~{e}) & {g})) + 0x{constant:016x}ull + w{slot};"
                )?;
                writeln!(
                    output,
                    "    unsigned long long temp2_{round} = big_sigma0({a}) + (({a} & {b}) ^ ({a} & {c}) ^ ({b} & {c}));"
                )?;
                writeln!(output, "    {d} = {d} + temp1_{round};")?;
                writeln!(output, "    {h} = temp1_{round} + temp2_{round};")?;
            }
        }
        state = [h, a, b, c, d, e, f, g];
    }

    match shader {
        Shader::Native => writeln!(
            output,
            "    return {} + 0x{:016x}lu;",
            state[0],
            sha512_constants::IV[0]
        )?,
        Shader::Web => {
            let high = sha512_constants::IV[0] >> 32;
            let low = sha512_constants::IV[0] & 0xffff_ffff;
            writeln!(
                output,
                "    return add64({}, vec2<u32>(0x{high:08x}u, 0x{low:08x}u));",
                state[0]
            )?;
        }
        Shader::Cuda => writeln!(
            output,
            "    return {} + 0x{:016x}ull;",
            state[0],
            sha512_constants::IV[0]
        )?,
    }

    Ok(output)
}
