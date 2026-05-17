use crate::WorkerMessage;



pub(crate) async fn compute_response_message(_request: WorkerMessage) -> WorkerMessage {
    WorkerMessage {
        msg_content: "todo".to_string(),
        msg_type:"todo".to_string(),
    }
}