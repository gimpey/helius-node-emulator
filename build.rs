fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .compile_protos(
            &[
                "protos/spl_token_creation.proto",
                "protos/daos_fund.proto",
            ],
            &["protos"],
        )?;
    Ok(())
}
