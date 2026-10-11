package cloud

import "testing"

func TestDashboardChatHistoryQueryLimit(t *testing.T) {
	for _, tc := range []struct{ requested, want int }{
		{120, 120}, {50, 50}, {1, 1}, {0, 30}, {121, 30}, {-1, 30},
	} {
		if got := dashboardChatHistoryQueryLimit(tc.requested); got != tc.want {
			t.Fatalf("limit %d = %d, want %d", tc.requested, got, tc.want)
		}
	}
}
