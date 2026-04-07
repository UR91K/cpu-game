# Logging Standards

## Required Logging Framework

This project uses `tracing` and `tracing-subscriber` for all logging and diagnostic output.

## Rules

1. **ALWAYS use `tracing` macros** for output:
   - `trace!()` - Very detailed information, typically only enabled in development
   - `debug!()` - Diagnostic information useful for debugging
   - `info!()` - General informational messages about application progress
   - `warn!()` - Warning messages for potentially problematic situations
   - `error!()` - Error messages for serious problems

2. **NEVER use standard output macros**:
   - ❌ `print!()`
   - ❌ `println!()`
   - ❌ `eprintln!()`
   - ❌ `dbg!()`

3. **Structured logging**: Use field syntax for structured data:
   ```rust
   info!(user_id = %user.id, action = "login", "User logged in");
   debug!(count = items.len(), "Processing items");
   error!(error = %e, "Failed to process request");
   ```

4. **Spans for context**: Use spans to group related operations:
   ```rust
   let span = info_span!("request", method = %req.method(), path = %req.path());
   let _enter = span.enter();
   ```

5. **Initialization**: Ensure `tracing-subscriber` is properly initialized at application startup:
   ```rust
   use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
   
   tracing_subscriber::registry()
       .with(tracing_subscriber::fmt::layer())
       .with(tracing_subscriber::EnvFilter::from_default_env())
       .init();
   ```

## Rationale

- Consistent, structured logging across the entire codebase
- Runtime control of log levels via environment variables (e.g., `RUST_LOG=debug`)
- Better integration with observability tools
- Contextual information through spans
- No mixing of logging mechanisms
