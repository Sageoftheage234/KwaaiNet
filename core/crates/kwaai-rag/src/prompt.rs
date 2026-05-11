use crate::retriever::RetrievedChunk;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Build a standalone RAG prompt string (for non-chat completions).
pub fn build_rag_prompt(
    user_query: &str,
    chunks: &[RetrievedChunk],
    max_context_chars: usize,
) -> String {
    let context = build_context_block(chunks, max_context_chars);
    let n = chunks.len();
    format!(
        "You are a research assistant. Use only the {n} source excerpt(s) below to answer.\n\
         Cite each fact with its source number in brackets, e.g. [1]. \
         If the answer is not in the sources, say so — do not fabricate.\n\n\
         Sources:\n{context}\n\n\
         Question: {user_query}\n\n\
         Answer:"
    )
}

/// Build a chat message list for `/v1/chat/completions`.
pub fn build_chat_messages(
    user_query: &str,
    chunks: &[RetrievedChunk],
    history: &[ChatMessage],
    max_context_chars: usize,
) -> Vec<ChatMessage> {
    let context = build_context_block(chunks, max_context_chars);

    let n = chunks.len();
    let system = ChatMessage {
        role: "system".to_string(),
        content: format!(
            "You are a research assistant with access to the following {n} source excerpt(s), \
             numbered [1]–[{n}].\n\n\
             Rules you must follow:\n\
             1. Every factual claim must be followed by its source number in brackets, \
                e.g. \"District Six was declared a White Group Area in 1966 [2].\"\n\
             2. If the answer requires information not present in the excerpts, say: \
                \"I don't have enough information in the provided sources to answer that.\"\n\
             3. Never fabricate facts, names, dates, or quotes.\n\
             4. If sources partially address the question, synthesise what they do say \
                and note the gap.\n\
             5. Write in clear, complete sentences suitable for a researcher.\n\n\
             Sources:\n{context}"
        ),
    };

    let mut messages = vec![system];
    messages.extend_from_slice(history);
    messages.push(ChatMessage {
        role: "user".to_string(),
        content: user_query.to_string(),
    });
    messages
}

fn build_context_block(chunks: &[RetrievedChunk], max_chars: usize) -> String {
    // Reorder to mitigate lost-in-the-middle: put even ranks at start, odd ranks
    // reversed at end. Best evidence lands at positions 1 and last where LLMs attend most.
    let reordered = reorder_for_context(chunks);
    let mut out = String::new();
    let mut used = 0usize;
    for (i, chunk) in reordered.iter().enumerate() {
        let text = if chunk.chunk_meta.surrounding.len() > chunk.chunk_meta.text.len() {
            &chunk.chunk_meta.surrounding
        } else {
            &chunk.chunk_meta.text
        };
        let entry = format!("[{}] {}\n", i + 1, text);
        if used + entry.len() > max_chars {
            break;
        }
        out.push_str(&entry);
        used += entry.len();
    }
    out
}

fn reorder_for_context(chunks: &[RetrievedChunk]) -> Vec<&RetrievedChunk> {
    let (evens, odds): (Vec<_>, Vec<_>) = chunks
        .iter()
        .enumerate()
        .partition(|(i, _)| i % 2 == 0);
    let mut result: Vec<&RetrievedChunk> = evens.into_iter().map(|(_, c)| c).collect();
    result.extend(odds.into_iter().rev().map(|(_, c)| c));
    result
}
