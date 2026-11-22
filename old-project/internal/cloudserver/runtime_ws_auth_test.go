package cloudserver

import (
	"errors"
	"testing"

	"github.com/0xmarkhydra/codelocal/internal/deviceauth"
	"github.com/coder/websocket"
)

func TestRuntimeDeviceProofCloseTaxonomy(t *testing.T) {
	code, reason := runtimeDeviceProofClose(deviceauth.ErrClockSkew)
	if code != websocket.StatusCode(4408) || reason != "DEVICE_CLOCK_SKEW" {
		t.Fatalf("clock skew mapped to %d %q", code, reason)
	}

	code, reason = runtimeDeviceProofClose(errors.New("invalid device signature"))
	if code != websocket.StatusCode(4402) || reason != "DEVICE_SIGNATURE_INVALID" {
		t.Fatalf("signature error mapped to %d %q", code, reason)
	}
}
