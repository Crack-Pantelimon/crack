use api_asscrack::crack_worker::WorkerMessage;

pub(crate) async fn compute_response_message(_request: WorkerMessage) -> WorkerMessage {
    tracing::info!("webworker compute_response_message()");
    WorkerMessage {
        msg_id: 666,
        msg_content: "todo".to_string().as_bytes().to_vec(),
        msg_type: "todo".to_string(),
    }
}
