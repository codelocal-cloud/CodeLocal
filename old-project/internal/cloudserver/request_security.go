package cloudserver

import (
	"errors"
	"net/http"
	"time"

	"github.com/0xmarkhydra/codelocal/internal/cloud"
	"github.com/0xmarkhydra/codelocal/internal/deviceauth"
	"github.com/0xmarkhydra/codelocal/internal/webutil"
)

type deviceProofError struct {
	err error
}

func (e *deviceProofError) Error() string {
	if e == nil || e.err == nil {
		return "device proof rejected"
	}
	return e.err.Error()
}

func (e *deviceProofError) Unwrap() error {
	if e == nil {
		return nil
	}
	return e.err
}

func deviceProofReason(err error) (string, bool) {
	var proofErr *deviceProofError
	if !errors.As(err, &proofErr) {
		return "", false
	}
	if errors.Is(proofErr, deviceauth.ErrClockSkew) {
		return "DEVICE_CLOCK_SKEW", true
	}
	return "DEVICE_SIGNATURE_INVALID", true
}

func (s *Server) writeDeviceAuthFailure(w http.ResponseWriter, device *cloud.Device, err error) bool {
	if err == nil && device != nil {
		return false
	}
	if reason, ok := deviceProofReason(err); ok {
		webutil.JSON(w, http.StatusConflict, map[string]any{
			"error":  "device_proof_failed",
			"reason": reason,
		})
		return true
	}
	webutil.JSON(w, http.StatusUnauthorized, map[string]any{"error": "device_auth_failed"})
	return true
}

func (s *Server) verifySignedDeviceRequest(r *http.Request, device *cloud.Device) error {
	if device == nil || device.PublicKey == "" {
		return nil
	}
	now := time.Now()
	if err := deviceauth.VerifyRequest(r, device.PublicKey, now); err != nil {
		return err
	}
	used, err := s.Store.ConsumeDeviceNonce(r.Context(), device.CredentialID, deviceauth.RequestNonce(r), now)
	if err != nil {
		return err
	}
	if !used {
		return errors.New("device signature replay detected")
	}
	return nil
}
