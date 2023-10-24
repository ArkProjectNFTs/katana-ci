use anyhow::Result;
use std::{env, sync::Arc};

use starknet::{
    accounts::{Account, ExecutionEncoding, SingleOwnerAccount},
    contract::ContractFactory,
    core::types::{contract::SierraClass, BlockId, BlockTag, FieldElement, FunctionCall},
    macros::{felt, selector},
    providers::{jsonrpc::HttpTransport, AnyProvider, JsonRpcClient, Provider},
    signers::{LocalWallet, SigningKey},
};
use tokio::time::{sleep, Duration};
use url::Url;

#[tokio::main]
async fn main() -> Result<()> {
    let rpc_url = env::var("STARKNET_RPC").expect("STARKNET_RPC must be set");
    let rpc_url = Url::parse(&rpc_url).expect("Invalid STARKNET_RPC URL");

    let provider =
        AnyProvider::JsonRpcHttp(JsonRpcClient::new(HttpTransport::new(rpc_url.clone())));

    // KATANA-0 account with default seed.
    let katana_chain_id = felt!("0x4b4154414e41");
    let katana_0_addr = FieldElement::from_hex_be(
        "0x517ececd29116499f4a1b64b094da79ba08dfd54a3edaa316134c41f8160973",
    )
    .unwrap();
    let katana_0_key =
        FieldElement::from_hex_be("0x1800000000300000180000000000030000000000003006001800006600")
            .unwrap();

    let signer = LocalWallet::from(SigningKey::from_secret_scalar(katana_0_key));

    let account = SingleOwnerAccount::new(
        provider,
        signer,
        katana_0_addr,
        katana_chain_id,
        ExecutionEncoding::Legacy,
    );

    println!("Declaring");
    let casm_class_hash = FieldElement::from_hex_be(
        "0x025dbb58db5071c88292cb25c81be128f2f47ccd8e3bd86260187f9937d181bb",
    )
    .unwrap();

    let class = serde_json::from_reader::<_, SierraClass>(std::fs::File::open(
        "./examples/contracts/c1.contract_class.json",
    )?)
    .expect("Failed to read sierra class");

    let class_hash = class.class_hash().unwrap();

    let declaration = account.declare(Arc::new(class.flatten()?), casm_class_hash);

    let _tx = match declaration.send().await {
        Ok(d) => Some(d.transaction_hash),
        Err(_e) => {
            // maybe already declared, skip for this example.
            None
        }
    };

    // Wait katana to process this tx, fairly quick. In production code
    // it's more reliable to poll the tx receipt with the tx hash.
    sleep(Duration::from_millis(2000)).await;

    println!("Deploying");
    let factory = ContractFactory::new(class_hash, account);

    let args = vec![];
    // Using a fix salt is usefull to have reproducible addresses
    // for testing.
    let salt = FieldElement::ZERO;
    let is_unique = false;
    let contract_deployment = factory.deploy(args, salt, is_unique);
    let deployed_address = contract_deployment.deployed_address();

    let _tx = contract_deployment.send().await?.transaction_hash;

    sleep(Duration::from_millis(2000)).await;

    let calldata = vec![];
    let block = BlockId::Tag(BlockTag::Pending);

    // account is for now consumed by the factory, and the provider
    // by the account and for now we can't get account from ContractFactory.
    println!("Calling");
    let provider =
        AnyProvider::JsonRpcHttp(JsonRpcClient::new(HttpTransport::new(rpc_url.clone())));

    let r = provider
        .call(
            FunctionCall {
                contract_address: deployed_address,
                entry_point_selector: selector!("say_hello"),
                calldata,
            },
            block,
        )
        .await?;

    assert_eq!(
        r[0],
        felt!("0x00000000000000000000000000000000000000000000000000000068656c6c6f")
    );

    println!("Call result: {:?}", r);

    // r[0] should be: 0x00000000000000000000000000000000000000000000000000000068656c6c6f, which is 'hello'.

    Ok(())
}
