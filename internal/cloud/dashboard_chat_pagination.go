package cloud

import (
	"context"
	"encoding/json"
	"strings"
)

// ListDashboardChatHistoryBefore reads older messages in one thread using a
// stable (created_at, id) cursor. It never loads or changes other users' data.
func (s *Store) ListDashboardChatHistoryBefore(ctx context.Context, userID, threadID string, beforeTime int64, beforeID string, limit int) ([]DashboardChatMessage, error) {
	limit = dashboardChatHistoryQueryLimit(limit)
	threadID = strings.TrimSpace(threadID)
	beforeID = strings.TrimSpace(beforeID)
	if threadID == "" || beforeID == "" || beforeTime <= 0 {
		return []DashboardChatMessage{}, nil
	}
	rows, err := s.DB.Query(ctx, `
SELECT id, user_id, thread_id, role, content, tool_calls, skills, image, created_at
FROM (
	SELECT id, user_id, thread_id, role, content, tool_calls, skills, image, created_at
	FROM codelocal_dashboard_chat
	WHERE user_id=$1 AND thread_id=$2 AND (created_at, id) < ($3, $4)
	ORDER BY created_at DESC, id DESC
	LIMIT $5
) recent
ORDER BY created_at ASC, id ASC`, userID, threadID, beforeTime, beforeID, limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := make([]DashboardChatMessage, 0, limit)
	for rows.Next() {
		var m DashboardChatMessage
		var threadIDVal *string
		var toolCalls, skills string
		var img *string
		if err := rows.Scan(&m.ID, &m.UserID, &threadIDVal, &m.Role, &m.Content, &toolCalls, &skills, &img, &m.CreatedAt); err != nil {
			return nil, err
		}
		if threadIDVal != nil {
			m.ThreadID = *threadIDVal
		}
		m.ToolCalls = json.RawMessage(toolCalls)
		m.Skills = json.RawMessage(skills)
		if img != nil {
			m.Image = *img
		}
		out = append(out, m)
	}
	return out, rows.Err()
}
