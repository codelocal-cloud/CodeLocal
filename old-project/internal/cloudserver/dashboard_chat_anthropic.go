package cloudserver

import (
	"bytes"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"strings"
	"time"

	"github.com/0xmarkhydra/codelocal/internal/cloud"
)

const dashboardAnthropicVersion = "2023-06-01"

func dashboardAnthropicImageSource(raw string) map[string]any {
	raw = strings.TrimSpace(raw)
	if strings.HasPrefix(raw, "data:") {
		if comma := strings.Index(raw, ","); comma > 5 {
			meta, data := raw[5:comma], raw[comma+1:]
			parts := strings.Split(meta, ";")
			if len(parts) > 0 && strings.Contains(meta, "base64") && strings.TrimSpace(parts[0]) != "" && data != "" {
				return map[string]any{"type": "base64", "media_type": strings.TrimSpace(parts[0]), "data": data}
			}
		}
	}
	return map[string]any{"type": "url", "url": raw}
}

func dashboardAnthropicContent(value any) []map[string]any {
	switch content := value.(type) {
	case string:
		if strings.TrimSpace(content) == "" {
			return nil
		}
		return []map[string]any{{"type": "text", "text": content}}
	case []map[string]any:
		out := make([]map[string]any, 0, len(content))
		for _, part := range content {
			switch part["type"] {
			case "text", "input_text":
				if text, _ := part["text"].(string); text != "" {
					out = append(out, map[string]any{"type": "text", "text": text})
				}
			case "image_url", "input_image":
				imageURL := ""
				if image, ok := part["image_url"].(map[string]any); ok {
					imageURL, _ = image["url"].(string)
				}
				if raw, ok := part["image_url"].(string); ok {
					imageURL = raw
				}
				if strings.TrimSpace(imageURL) != "" {
					out = append(out, map[string]any{"type": "image", "source": dashboardAnthropicImageSource(imageURL)})
				}
			}
		}
		return out
	default:
		return nil
	}
}

func dashboardAnthropicMessages(messages []map[string]any) (string, []map[string]any) {
	system := ""
	out := []map[string]any{}
	appendMessage := func(role string, parts []map[string]any) {
		if len(parts) == 0 {
			return
		}
		if len(out) > 0 && out[len(out)-1]["role"] == role {
			previous, _ := out[len(out)-1]["content"].([]map[string]any)
			out[len(out)-1]["content"] = append(previous, parts...)
			return
		}
		out = append(out, map[string]any{"role": role, "content": parts})
	}
	for _, message := range messages {
		role, _ := message["role"].(string)
		switch role {
		case "system":
			if text, _ := message["content"].(string); strings.TrimSpace(text) != "" {
				if system != "" {
					system += "\n\n"
				}
				system += text
			}
		case "user":
			appendMessage("user", dashboardAnthropicContent(message["content"]))
		case "assistant":
			parts := dashboardAnthropicContent(message["content"])
			if calls, ok := message["tool_calls"].([]map[string]any); ok {
				for _, call := range calls {
					fn, _ := call["function"].(map[string]any)
					id, _ := call["id"].(string)
					name, _ := fn["name"].(string)
					arguments, _ := fn["arguments"].(string)
					input := map[string]any{}
					_ = json.Unmarshal([]byte(arguments), &input)
					if id != "" && name != "" {
						parts = append(parts, map[string]any{"type": "tool_use", "id": id, "name": name, "input": input})
					}
				}
			}
			appendMessage("assistant", parts)
		case "tool":
			id, _ := message["tool_call_id"].(string)
			content, _ := message["content"].(string)
			if id != "" {
				appendMessage("user", []map[string]any{{"type": "tool_result", "tool_use_id": id, "content": content}})
			}
		}
	}
	return system, out
}

func dashboardAnthropicTools(tools []map[string]any) []map[string]any {
	out := make([]map[string]any, 0, len(tools))
	for _, tool := range tools {
		fn, _ := tool["function"].(map[string]any)
		name, _ := fn["name"].(string)
		if name == "" {
			continue
		}
		item := map[string]any{"name": name, "input_schema": fn["parameters"]}
		if description, _ := fn["description"].(string); description != "" {
			item["description"] = description
		}
		out = append(out, item)
	}
	return out
}

func callAnthropicMessagesWithTools(baseURL, apiKey, model string, messages []map[string]any, tools []map[string]any) ([]llmToolCall, string, error) {
	system, converted := dashboardAnthropicMessages(messages)
	body := map[string]any{"model": model, "max_tokens": 4096, "messages": converted}
	if system != "" {
		body["system"] = system
	}
	if convertedTools := dashboardAnthropicTools(tools); len(convertedTools) > 0 {
		body["tools"] = convertedTools
	}
	encoded, _ := json.Marshal(body)
	req, err := http.NewRequest(http.MethodPost, strings.TrimRight(baseURL, "/")+"/messages", bytes.NewReader(encoded))
	if err != nil {
		return nil, "", err
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Accept", "application/json")
	req.Header.Set("x-api-key", apiKey)
	req.Header.Set("anthropic-version", dashboardAnthropicVersion)
	resp, err := dashboardLLMHTTPClient(60 * time.Second).Do(req)
	if err != nil {
		return nil, "", err
	}
	defer resp.Body.Close()
	raw, err := io.ReadAll(io.LimitReader(resp.Body, 8<<20))
	if err != nil {
		return nil, "", err
	}
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return nil, "", &httpError{Status: resp.StatusCode, Body: string(raw)}
	}
	var response struct {
		Content []struct {
			Type  string          `json:"type"`
			Text  string          `json:"text"`
			ID    string          `json:"id"`
			Name  string          `json:"name"`
			Input json.RawMessage `json:"input"`
		} `json:"content"`
	}
	if err := json.Unmarshal(raw, &response); err != nil {
		return nil, "", err
	}
	var text strings.Builder
	calls := []llmToolCall{}
	for _, part := range response.Content {
		switch part.Type {
		case "text":
			text.WriteString(part.Text)
		case "tool_use":
			arguments := string(part.Input)
			if strings.TrimSpace(arguments) == "" {
				arguments = "{}"
			}
			calls = append(calls, llmToolCall{ID: part.ID, Name: part.Name, Arguments: arguments})
		}
	}
	return calls, text.String(), nil
}

func proxyAnthropicMessagesStream(w http.ResponseWriter, flusher http.Flusher, baseURL, apiKey, model string, messages []map[string]any, tools []map[string]any, r *http.Request, s *Server, userID string) error {
	follow := append([]map[string]any{}, messages...)
	resume := dashboardExecutionResumeFromRequest(r)
	allResults := append(make([]dashboardToolCall, 0, dashboardMaxToolCalls), resume.Results...)
	seenProgress := dashboardSeedToolProgress(allResults)
	stopReason, finalContent := "", ""
	for round := 0; round < dashboardMaxToolRounds && len(allResults) < dashboardMaxToolCalls; round++ {
		calls, content, err := callAnthropicMessagesWithTools(baseURL, apiKey, model, follow, tools)
		if err != nil {
			if len(allResults) == 0 {
				return &dashboardSafeRerouteError{Err: err}
			}
			stopReason = "model connection interrupted after completed tool work"
			break
		}
		if len(calls) == 0 {
			finalContent = content
			break
		}
		if err := dashboardValidateToolCalls(calls); err != nil {
			return err
		}
		results := make([]dashboardToolCall, 0, len(calls))
		toolCallsAny := make([]map[string]any, 0, len(calls))
		noProgress := false
		for _, call := range calls {
			if len(allResults) >= dashboardMaxToolCalls {
				stopReason = "tool execution budget reached"
				break
			}
			args := map[string]any{}
			_ = json.Unmarshal([]byte(call.Arguments), &args)
			started := time.Now()
			resultText := execDashboardTool(r, s, userID, call.Name, args)
			result := dashboardToolCall{ID: call.ID, Name: call.Name, Arguments: call.Arguments, Result: resultText, DurationMs: time.Since(started).Milliseconds(), Status: dashboardToolResultStatus(resultText)}
			results = append(results, result)
			allResults = append(allResults, result)
			toolCallsAny = append(toolCallsAny, map[string]any{"id": call.ID, "type": "function", "function": map[string]any{"name": call.Name, "arguments": call.Arguments}})
			fingerprint := dashboardToolProgressFingerprint(call, resultText)
			seenProgress[fingerprint]++
			if seenProgress[fingerprint] >= dashboardDuplicateResultLimit {
				noProgress = true
			}
		}
		if len(results) > 0 {
			writeDashboardSSE(w, flusher, "tool_calls", map[string]any{"tool_calls": allResults})
		}
		follow = append(follow, map[string]any{"role": "assistant", "content": content, "tool_calls": toolCallsAny})
		for _, result := range results {
			follow = append(follow, map[string]any{"role": "tool", "content": result.Result, "tool_call_id": result.ID, "name": result.Name})
		}
		s.saveDashboardChatToolCheckpoint(r, userID, allResults)
		if stopReason != "" {
			break
		}
		if noProgress {
			stopReason = "repeated tool calls produced no new result"
			break
		}
	}
	if finalContent == "" {
		if stopReason == "" {
			stopReason = "tool execution budget reached"
		}
		_, synthesized, err := callAnthropicMessagesWithTools(baseURL, apiKey, model, dashboardFinalSynthesisMessages(follow, stopReason), nil)
		if err == nil && strings.TrimSpace(synthesized) != "" {
			finalContent = synthesized
		} else {
			finalContent = dashboardFallbackReply(allResults, stopReason)
		}
	}
	if strings.TrimSpace(finalContent) == "" {
		return errors.New("anthropic provider returned an empty response")
	}
	writeDashboardTextDeltas(w, flusher, finalContent)
	tcsJSON, _ := json.Marshal(allResults)
	_ = s.saveDashboardChatMessageDurably(r, cloud.DashboardChatMessage{ID: dashboardChatMessageID(r, userID, "assistant"), UserID: userID, Role: "assistant", Content: finalContent, ToolCalls: json.RawMessage(tcsJSON), CreatedAt: time.Now().UnixMilli()})
	s.recordDashboardChatUsage(r, userID, model, "anthropic_messages", dashboardChatTokenUsage{})
	writeDashboardSSE(w, flusher, "done", map[string]any{"done": true, "reply": finalContent, "tool_calls": allResults, "model": dashboardPublicModelName, "threadId": dashboardChatThreadID(r)})
	return nil
}
