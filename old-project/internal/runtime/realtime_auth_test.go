package runtime

import (
	"errors"
	"testing"

	"github.com/coder/websocket"
)

func TestRuntimeControlErrorUsesLegacyCloseReasonBeforeGeneric4403(t *testing.T) {
	err := websocket.CloseError{Code: websocket.StatusCode(4403), Reason: "DEVICE_SIGNATURE_INVALID"}
	if got := runtimeControlError(err); !errors.Is(got, ErrDeviceProofRejected) {
		t.Fatalf("legacy 4403 signature reason mapped to %v", got)
	}
}

func TestRuntimeControlStatusErrorTaxonomy(t *testing.T) {
	tests := []struct {
		status websocket.StatusCode
		want   error
	}{
		{status: websocket.StatusCode(4403), want: ErrDeviceAuthorizationRevoked},
		{status: websocket.StatusCode(4402), want: ErrDeviceProofRejected},
		{status: websocket.StatusCode(4408), want: ErrDeviceClockSkew},
		{status: websocket.StatusCode(4409), want: ErrDeviceIdentityMismatch},
	}
	for _, test := range tests {
		if got := runtimeControlStatusError(test.status); !errors.Is(got, test.want) {
			t.Fatalf("status %d: got %v, want %v", test.status, got, test.want)
		}
	}
	if got := runtimeControlStatusError(websocket.StatusNormalClosure); got != nil {
		t.Fatalf("normal closure must not map to a device-auth error: %v", got)
	}
}
