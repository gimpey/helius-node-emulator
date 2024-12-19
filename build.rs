fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .compile_protos(
            &[
                "protos/spl_token.proto",
                "protos/daos_fund.proto",
                "protos/system.proto",
            ],
            &["protos"],
        )?;
    Ok(())
}
