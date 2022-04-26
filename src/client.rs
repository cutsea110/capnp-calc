use crate::calculator_capnp::calculator;
use capnp::capability::Promise;
use capnp_rpc::{rpc_twoparty_capnp, twoparty, RpcSystem};
use futures::{AsyncReadExt, FutureExt};

#[derive(Debug, Clone, Copy)]
pub struct PowerFunction;

impl calculator::function::Server for PowerFunction {
    fn call(
        &mut self,
        params: calculator::function::CallParams,
        mut results: calculator::function::CallResults,
    ) -> Promise<(), capnp::Error> {
        let params = pry!(pry!(params.get()).get_params());
        if params.len() != 2 {
            Promise::err(::capnp::Error::failed(
                "Wrong number of parameters".to_string(),
            ))
        } else {
            results.get().set_value(params.get(0).powf(params.get(1)));
            Promise::ok(())
        }
    }
}

pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = ::std::env::args().collect();
    if args.len() != 3 {
        println!("usage: {} client HOST:PORT", args[0]);
        return Ok(());
    }
    tokio::task::LocalSet::new().run_until(try_main(args)).await
}

async fn try_main(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    use std::net::ToSocketAddrs;

    let addr = args[2]
        .to_socket_addrs()?
        .next()
        .expect("could not parse address");
    let stream = tokio::net::TcpStream::connect(&addr).await?;
    stream.set_nodelay(true)?;
    let (reader, writer) = tokio_util::compat::TokioAsyncReadCompatExt::compat(stream).split();

    let network = Box::new(twoparty::VatNetwork::new(
        reader,
        writer,
        rpc_twoparty_capnp::Side::Client,
        Default::default(),
    ));

    let mut rpc_system = RpcSystem::new(network, None);
    let calculator: calculator::Client = rpc_system.bootstrap(rpc_twoparty_capnp::Side::Server);
    tokio::task::spawn_local(Box::pin(rpc_system.map(|_| ())));

    {
        // literal 123.0

        println!("Evaluating a literal...");
        let mut eval_request = calculator.evaluate_request();
        eval_request.get().init_expression().set_literal(123.0);
        let value = eval_request.send().pipeline.get_value();
        let read_request = value.read_request();
        let response = read_request.send().promise.await?;
        assert_eq!(response.get()?.get_value(), 123.0);
        println!("PASS");
    }

    {
        // 123 + 45 - 67

        println!("Using add and subtract... ");
        let add = {
            let mut request = calculator.get_operator_request();
            request.get().set_op(calculator::Operator::Add);
            request.send().pipeline.get_func()
        };
        let subtract = {
            let mut request = calculator.get_operator_request();
            request.get().set_op(calculator::Operator::Subtract);
            request.send().pipeline.get_func()
        };

        let mut request = calculator.evaluate_request();

        {
            let mut subtract_call = request.get().init_expression().init_call();
            subtract_call.set_function(subtract);
            let mut subtract_params = subtract_call.init_params(2);
            subtract_params.reborrow().get(1).set_literal(67.0);

            let mut add_call = subtract_params.get(0).init_call();
            add_call.set_function(add);
            let mut add_params = add_call.init_params(2);
            add_params.reborrow().get(0).set_literal(123.0);
            add_params.get(1).set_literal(45.0);
        }

        let eval_promise = request.send();
        let read_promise = eval_promise.pipeline.get_value().read_request().send();

        let response = read_promise.promise.await?;
        assert_eq!(response.get()?.get_value(), 101.0);
        println!("PASS");
    }

    {
        // 4 * 6 + 3
        // 4 * 6 + 5

        println!("Pipelining eval() calls... ");

        let add = {
            let mut request = calculator.get_operator_request();
            request.get().set_op(calculator::Operator::Add);
            request.send().pipeline.get_func()
        };
        let multiply = {
            let mut request = calculator.get_operator_request();
            request.get().set_op(calculator::Operator::Multiply);
            request.send().pipeline.get_func()
        };

        let mut request = calculator.evaluate_request();

        {
            let mut multiply_call = request.get().init_expression().init_call();
            multiply_call.set_function(multiply);
            let mut multiply_params = multiply_call.init_params(2);
            multiply_params.reborrow().get(0).set_literal(4.0);
            multiply_params.reborrow().get(1).set_literal(6.0);
        }

        let multiply_result = request.send().pipeline.get_value();

        let mut add3_request = calculator.evaluate_request();
        {
            let mut add3_call = add3_request.get().init_expression().init_call();
            add3_call.set_function(add.clone());
            let mut add3_params = add3_call.init_params(2);
            add3_params
                .reborrow()
                .get(0)
                .set_previous_result(multiply_result.clone());
            add3_params.reborrow().get(1).set_literal(3.0);
        }

        let add3_promise = add3_request
            .send()
            .pipeline
            .get_value()
            .read_request()
            .send();

        let mut add5_request = calculator.evaluate_request();
        {
            let mut add5_call = add5_request.get().init_expression().init_call();
            add5_call.set_function(add.clone());
            let mut add5_params = add5_call.init_params(2);
            add5_params
                .reborrow()
                .get(0)
                .set_previous_result(multiply_result);
            add5_params.reborrow().get(1).set_literal(5.0);
        }

        let add5_promise = add5_request
            .send()
            .pipeline
            .get_value()
            .read_request()
            .send();

        assert_eq!(add3_promise.promise.await?.get()?.get_value(), 27.0);
        assert_eq!(add5_promise.promise.await?.get()?.get_value(), 29.0);

        println!("PASS");
    }

    {
        // f(x, y) = x * 100 + y
        // g(x) = f(x, x+1) * 2
        // f(12, 34)
        // g(21)
        println!("Defining functions... ");

        let add = {
            let mut request = calculator.get_operator_request();
            request.get().set_op(calculator::Operator::Add);
            request.send().pipeline.get_func()
        };
        let multiply = {
            let mut request = calculator.get_operator_request();
            request.get().set_op(calculator::Operator::Multiply);
            request.send().pipeline.get_func()
        };

        let f = {
            let mut request = calculator.def_function_request();
            {
                let mut def_function_params = request.get();
                def_function_params.set_param_count(2);
                {
                    let mut add_call = def_function_params.init_body().init_call();
                    add_call.set_function(add.clone());
                    let mut add_params = add_call.init_params(2);
                    add_params.reborrow().get(1).set_parameter(1);

                    let mut multiply_call = add_params.get(0).init_call();
                    multiply_call.set_function(multiply.clone());
                    let mut multiply_params = multiply_call.init_params(2);
                    multiply_params.reborrow().get(0).set_parameter(0);
                    multiply_params.get(1).set_literal(100.0);
                }
            }
            request.send().pipeline.get_func()
        };

        let g = {
            let mut request = calculator.def_function_request();
            {
                let mut def_function_params = request.get();
                def_function_params.set_param_count(1);
                {
                    let mut multiply_call = def_function_params.init_body().init_call();
                    multiply_call.set_function(multiply);
                    let mut multiply_params = multiply_call.init_params(2);
                    multiply_params.reborrow().get(1).set_literal(2.0);

                    let mut f_call = multiply_params.get(0).init_call();
                    f_call.set_function(f.clone());
                    let mut f_params = f_call.init_params(2);
                    f_params.reborrow().get(0).set_parameter(0);

                    let mut add_call = f_params.get(1).init_call();
                    add_call.set_function(add);
                    let mut add_params = add_call.init_params(2);
                    add_params.reborrow().get(0).set_parameter(0);
                    add_params.get(1).set_literal(1.0);
                }
            }
            request.send().pipeline.get_func()
        };

        let mut f_eval_request = calculator.evaluate_request();
        {
            let mut f_call = f_eval_request.get().init_expression().init_call();
            f_call.set_function(f);
            let mut f_params = f_call.init_params(2);
            f_params.reborrow().get(0).set_literal(12.0);
            f_params.get(1).set_literal(34.0);
        }
        let f_eval_promise = f_eval_request
            .send()
            .pipeline
            .get_value()
            .read_request()
            .send();

        let mut g_eval_request = calculator.evaluate_request();
        {
            let mut g_call = g_eval_request.get().init_expression().init_call();
            g_call.set_function(g);
            g_call.init_params(1).get(0).set_literal(21.0);
        }
        let g_eval_promise = g_eval_request
            .send()
            .pipeline
            .get_value()
            .read_request()
            .send();

        assert_eq!(f_eval_promise.promise.await?.get()?.get_value(), 1234.0);
        assert_eq!(g_eval_promise.promise.await?.get()?.get_value(), 4244.0);

        println!("PASS")
    }

    {
        println!("Using a callback... ");

        let add = {
            let mut request = calculator.get_operator_request();
            request.get().set_op(calculator::Operator::Add);
            request.send().pipeline.get_func()
        };

        let mut request = calculator.evaluate_request();
        {
            let mut pow_call = request.get().init_expression().init_call();
            pow_call.set_function(capnp_rpc::new_client(PowerFunction));
            let mut pow_params = pow_call.init_params(2);
            pow_params.reborrow().get(0).set_literal(2.0);

            let mut add_call = pow_params.get(1).init_call();
            add_call.set_function(add);
            let mut add_params = add_call.init_params(2);
            add_params.reborrow().get(0).set_literal(4.0);
            add_params.get(1).set_literal(5.0);
        }

        let response_promise = request.send().pipeline.get_value().read_request().send();

        assert_eq!(response_promise.promise.await?.get()?.get_value(), 512.0);

        println!("PASS");
    }

    Ok(())
}
