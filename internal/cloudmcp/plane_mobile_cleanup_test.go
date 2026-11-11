package cloudmcp

import "testing"

func TestGeneratedMobileTaskRequiresExactSourceKeyTitleAndID(t *testing.T) {
	base := map[string]any{
		"id":              "b3ca30f6-5108-4df3-96b9-62f1a983dea3",
		"external_source": "codelocal-chatgpt-mobile-sprint",
		"external_id":     "S1-MOB-01",
		"name":            "[S1-MOB-01] Bảo toàn code ChatNet và đồng bộ main",
	}
	if !isGeneratedMobileTask(base) {
		t.Fatal("legitimate task must be recognized")
	}
	for _, tc := range []struct {
		name  string
		field string
		value string
	}{
		{"wrong source", "external_source", "another-creator"},
		{"wrong key", "external_id", "S1-MOB-23"},
		{"wrong prefix", "external_id", "S1-MOB-1"},
		{"wrong name", "name", "[S1-MOB-02] Different task"},
		{"missing id", "id", ""},
	} {
		t.Run(tc.name, func(t *testing.T) {
			m := map[string]any{}
			for key, value := range base {
				m[key] = value
			}
			m[tc.field] = tc.value
			if isGeneratedMobileTask(m) {
				t.Fatal("cleanup matched non-target work item")
			}
		})
	}
}
