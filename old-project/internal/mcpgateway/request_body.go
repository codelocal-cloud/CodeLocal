package mcpgateway

import (
	"bytes"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"strings"
)

var errMCPRequestBodyTooLarge = errors.New("MCP request body too large")

type replayReadCloser struct {
	io.Reader
	io.Closer
}

func readBoundedMCPRequestBody(r *http.Request) ([]byte, error) {
	if r == nil || r.Body == nil {
		return nil, io.ErrUnexpectedEOF
	}
	if r.ContentLength > legacyToolCompatibilityMaxBody {
		return nil, errMCPRequestBodyTooLarge
	}

	originalBody := r.Body
	originalLength := r.ContentLength
	raw, err := io.ReadAll(io.LimitReader(originalBody, legacyToolCompatibilityMaxBody+1))
	if err != nil {
		r.Body = &replayReadCloser{Reader: io.MultiReader(bytes.NewReader(raw), originalBody), Closer: originalBody}
		r.ContentLength = originalLength
		return nil, err
	}
	if len(raw) > legacyToolCompatibilityMaxBody {
		// Preserve the unread remainder so observability/compatibility peeks never
		// truncate a chunked request before the real handler sees it.
		r.Body = &replayReadCloser{Reader: io.MultiReader(bytes.NewReader(raw), originalBody), Closer: originalBody}
		r.ContentLength = originalLength
		return nil, errMCPRequestBodyTooLarge
	}

	_ = originalBody.Close()
	r.Body = io.NopCloser(bytes.NewReader(raw))
	r.ContentLength = int64(len(raw))
	return raw, nil
}

func MCPRequestMethod(r *http.Request) string {
	if r == nil || r.URL.Path != "/mcp" || r.Method != http.MethodPost || r.Body == nil {
		return ""
	}
	raw, err := readBoundedMCPRequestBody(r)
	if err != nil {
		return ""
	}
	var envelope any
	decoder := json.NewDecoder(bytes.NewReader(raw))
	decoder.UseNumber()
	if decoder.Decode(&envelope) != nil {
		return ""
	}
	methods := make([]string, 0, 2)
	var collect func(any)
	collect = func(value any) {
		switch typed := value.(type) {
		case []any:
			for _, item := range typed {
				collect(item)
			}
		case map[string]any:
			if method, _ := typed["method"].(string); method != "" {
				methods = append(methods, method)
			}
		}
	}
	collect(envelope)
	return strings.Join(methods, ",")
}
