package cloudserver

import (
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"github.com/0xmarkhydra/codelocal/internal/cloud"
	"github.com/0xmarkhydra/codelocal/internal/deviceauth"
)

func TestWriteDeviceAuthFailureSeparatesProofFromCredentialFailure(t *testing.T) {
	t.Run("clock skew is a non-revocation conflict", func(t *testing.T) {
		recorder := httptest.NewRecorder()
		server := &Server{}
		if !server.writeDeviceAuthFailure(recorder, &cloud.Device{}, &deviceProofError{err: deviceauth.ErrClockSkew}) {
			t.Fatal("expected auth failure to be written")
		}
		if recorder.Code != http.StatusConflict {
			t.Fatalf("status=%d want %d", recorder.Code, http.StatusConflict)
		}
		if !strings.Contains(recorder.Body.String(), "DEVICE_CLOCK_SKEW") {
			t.Fatalf("missing clock-skew reason: %s", recorder.Body.String())
		}
	})

	t.Run("invalid signature is a non-revocation conflict", func(t *testing.T) {
		recorder := httptest.NewRecorder()
		server := &Server{}
		if !server.writeDeviceAuthFailure(recorder, &cloud.Device{}, &deviceProofError{err: errors.New("invalid signature")}) {
			t.Fatal("expected auth failure to be written")
		}
		if recorder.Code != http.StatusConflict {
			t.Fatalf("status=%d want %d", recorder.Code, http.StatusConflict)
		}
		if !strings.Contains(recorder.Body.String(), "DEVICE_SIGNATURE_INVALID") {
			t.Fatalf("missing signature reason: %s", recorder.Body.String())
		}
	})

	t.Run("missing credential stays unauthorized", func(t *testing.T) {
		recorder := httptest.NewRecorder()
		server := &Server{}
		if !server.writeDeviceAuthFailure(recorder, nil, nil) {
			t.Fatal("expected auth failure to be written")
		}
		if recorder.Code != http.StatusUnauthorized {
			t.Fatalf("status=%d want %d", recorder.Code, http.StatusUnauthorized)
		}
	})
}
