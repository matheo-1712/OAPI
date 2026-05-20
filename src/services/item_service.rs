use tracing::debug;

/// Example of a logic-heavy service function
pub fn process_item_logic(name: &str) -> String {
    debug!("Executing complex logic for: {}", name);
    // Imagine complex algorithms, data transformations, or external service calls here
    format!("LOGIC_PROCESSED: {}", name.to_uppercase())
}
