@0xce7140923b951bd3;

interface Calculator {
  evaluate @0 (expression :Expression) -> (value :Value);
  # Evaluate the given expression and return the result.
  # The result is returned wrapped in a Value interface so that you
  # may pass it back to the server in a pipelined request.
  # To actually get the numeric value, you must call read() on
  # the Value -- but again, this can be pipelined so that it incurs
  # no additional latency.
  
  struct Expression {
    union {
      literal @0 :Float64;
      # A literal numeric value.
      
      previousResult @1 :Value;
      # A value that was (or, will be) returned by a previous
      # evaluate().
      
      parameter @2 :UInt32;
      # A parameter to the function (only valid in function bodies;
      # see defFunction).
  
      call :group {
      	# Call a function on a list of parameters.
        function @3 :Function;
        params @4 :List(Expression);
      }
    }
  }

  interface Value {
    # Wraps a numeric value in an RPC object.
    # This allows the value to be used in subsequent evaluate() requests
    # without the client waiting for the evaluate() that returns the Value to finish.
    read @0 () -> (value :Float64);
    # Read back the raw numeric value.
  }

  defFunction @1 (paramCount :Int32, body :Expression) -> (func :Function);
  # Define a function that takes `paramCount` parameters and returns
  # the evaluation of `body` after substituting these parameters.

  interface Function {
    # An algebraic function. Can be called directly, or can be used inside an Expression.
    #
    # A client can create a Function that runs on the server side using `defFunction()`
    # or `getOperator()`. Alternatively, a client can implement a Function on the client side
    # and the server will call back to it.
    # However, a function defined on the client side will require a network round trip
    # whenever the server needs to call it, whereas functions defined on the server
    # and then passed back to it are called locally.

    call @0 (params :List(Float64)) -> (value :Float64);
    # Call the function on the given parameters.
  }

  getOperator @2 (op :Operator) -> (func :Function);
  # Get a Function representing an arithmetic operator, which can then be used in Expression.

  enum Operator {
    add @0;
    subtract @1;
    multiply @2;
    divide @3;
  }
}
