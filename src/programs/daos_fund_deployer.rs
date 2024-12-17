use bs58;

pub struct DaosFundDeployerAddresses {
    pub program_id: &'static str,
}

pub struct DaosFundDeployerDiscriminators {
    pub initialize_curve: u64,
}

pub struct DaosFundDeployer {
    pub addresses: DaosFundDeployerAddresses,
    pub discriminators: DaosFundDeployerDiscriminators,
}

pub enum DaosFundDeployerFunction {
    InitializeCurve,
}

pub const DAOS_FUND_DEPLOYER: DaosFundDeployer = DaosFundDeployer {
    addresses: DaosFundDeployerAddresses {
        program_id: "4FqThZWv3QKWkSyXCDmATpWkpEiCHq5yhkdGWpSEDAZM  ",
    },
    discriminators: DaosFundDeployerDiscriminators {
        initialize_curve: 0x265d01d63bb94c59,
    },
};

impl DaosFundDeployerFunction {
    pub fn from_data(data: &str) -> Option<DaosFundDeployerFunction> {
        let bytes = bs58::decode(data).into_vec().ok()?;

        if data.len() < 8 {
            return None;
        }

        let discriminator = u64::from_be_bytes(bytes[0..8].try_into().unwrap());
        match discriminator {
            x if x == DAOS_FUND_DEPLOYER.discriminators.initialize_curve => Some(DaosFundDeployerFunction::InitializeCurve),
            _ => None,
        }
    }
}