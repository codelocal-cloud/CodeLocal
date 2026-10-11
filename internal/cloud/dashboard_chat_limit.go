package cloud

// The execution layer requests 120 recent rows; don't silently downgrade that
// request to 30. This is a query cap, not a history retention limit.
func dashboardChatHistoryQueryLimit(limit int) int {
	if limit <= 0 || limit > 120 {
		return 30
	}
	return limit
}
