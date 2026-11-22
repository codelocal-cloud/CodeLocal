package cloud

import (
	"context"
	"errors"
	"strings"
	"time"

	"github.com/jackc/pgx/v5"
)

const workspaceRoutingPreferenceMigrationSQL = `
CREATE TABLE IF NOT EXISTS codelocal_workspace_routing_preferences (
 user_id TEXT PRIMARY KEY REFERENCES codelocal_users(id) ON DELETE CASCADE,
 default_workspace_key TEXT NOT NULL DEFAULT '',
 updated_at BIGINT NOT NULL
);
`

type WorkspaceRoutingPreference struct {
	DefaultWorkspaceKey string `json:"defaultWorkspaceKey,omitempty"`
	UpdatedAt           int64  `json:"updatedAt,omitempty"`
}

func (s *Store) WorkspaceRoutingPreference(ctx context.Context, userID string) (WorkspaceRoutingPreference, error) {
	if s == nil || s.DB == nil {
		return WorkspaceRoutingPreference{}, errors.New("workspace routing preference store unavailable")
	}
	userID = strings.TrimSpace(userID)
	if userID == "" {
		return WorkspaceRoutingPreference{}, errors.New("workspace routing preference requires user")
	}
	var preference WorkspaceRoutingPreference
	err := s.DB.QueryRow(ctx, `SELECT default_workspace_key,updated_at FROM codelocal_workspace_routing_preferences WHERE user_id=$1`, userID).Scan(
		&preference.DefaultWorkspaceKey, &preference.UpdatedAt,
	)
	if errors.Is(err, pgx.ErrNoRows) {
		return WorkspaceRoutingPreference{}, nil
	}
	return preference, err
}

func (s *Store) SetDefaultWorkspaceKey(ctx context.Context, userID, workspaceKey string) error {
	if s == nil || s.DB == nil {
		return errors.New("workspace routing preference store unavailable")
	}
	userID = strings.TrimSpace(userID)
	workspaceKey = strings.TrimSpace(workspaceKey)
	if userID == "" || workspaceKey == "" {
		return errors.New("user and workspace key are required")
	}
	_, err := s.DB.Exec(ctx, `
INSERT INTO codelocal_workspace_routing_preferences(user_id,default_workspace_key,updated_at)
VALUES($1,$2,$3)
ON CONFLICT(user_id) DO UPDATE SET default_workspace_key=EXCLUDED.default_workspace_key,updated_at=EXCLUDED.updated_at
`, userID, workspaceKey, time.Now().UnixMilli())
	return err
}
