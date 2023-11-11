use std::io::{self, Write};

use ndarray::{array, concatenate, s, Array1, Axis};
use ort::{download::language::machine_comprehension::GPT2, inputs, CUDAExecutionProvider, Environment, GraphOptimizationLevel, SessionBuilder, Tensor};
use rand::Rng;
use tokenizers::Tokenizer;

const PROMPT: &str = "The corsac fox (Vulpes corsac), also known simply as a corsac, is a medium-sized fox found in";
const GEN_TOKENS: i32 = 90;
const TOP_K: usize = 5;

fn main() -> ort::Result<()> {
	tracing_subscriber::fmt::init();

	let mut stdout = io::stdout();
	let mut rng = rand::thread_rng();

	let environment = Environment::builder()
		.with_name("GPT-2")
		.with_execution_providers([CUDAExecutionProvider::default().build()])
		.build()?
		.into_arc();

	let session = SessionBuilder::new(&environment)?
		.with_optimization_level(GraphOptimizationLevel::Level1)?
		.with_intra_threads(1)?
		.with_model_downloaded(GPT2::GPT2LmHead)?;

	let tokenizer = Tokenizer::from_file("tests/data/gpt2-tokenizer.json").unwrap();
	let tokens = tokenizer.encode(PROMPT, false).unwrap();
	let tokens = tokens.get_ids().iter().map(|i| *i as i64).collect::<Vec<_>>();

	let mut tokens = Array1::from_iter(tokens.iter().cloned());

	print!("{PROMPT}");
	stdout.flush().unwrap();

	for _ in 0..GEN_TOKENS {
		let array = tokens.view().insert_axis(Axis(0)).insert_axis(Axis(1));
		let outputs = session.run(inputs![array]?)?;
		let generated_tokens: Tensor<f32> = outputs["output1"].extract_tensor()?;
		let generated_tokens = generated_tokens.view();

		let probabilities = &mut generated_tokens
			.slice(s![0, 0, -1, ..])
			.insert_axis(Axis(0))
			.to_owned()
			.iter()
			.cloned()
			.enumerate()
			.collect::<Vec<_>>();
		probabilities.sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Less));

		let token = probabilities[rng.gen_range(0..=TOP_K)].0;
		tokens = concatenate![Axis(0), tokens, array![token.try_into().unwrap()]];

		let token_str = tokenizer.decode(&[token as _], true).unwrap();
		print!("{}", token_str);
		stdout.flush().unwrap();
	}

	println!();

	Ok(())
}
