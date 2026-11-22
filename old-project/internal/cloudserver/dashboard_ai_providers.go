package cloudserver

import (
	"context"
	"errors"
	"fmt"
	"net"
	"net/http"
	"net/url"
	"strings"
	"time"

	"github.com/0xmarkhydra/codelocal/internal/cloud"
	"github.com/0xmarkhydra/codelocal/internal/webutil"
	"github.com/jackc/pgx/v5"
)

const dashboardUserModelPrefix = "byo:"

type dashboardAIProviderMutation struct {
	Name        string            `json:"name"`
	BaseURL     string            `json:"baseUrl"`
	APIKey      string            `json:"apiKey,omitempty"`
	Protocol    string            `json:"protocol,omitempty"`
	Models      []string          `json:"models,omitempty"`
	ModelLabels map[string]string `json:"modelLabels,omitempty"`
	Enabled     *bool             `json:"enabled,omitempty"`
}

type dashboardModelOption struct {
	ID         string `json:"id"`
	Label      string `json:"label"`
	Provider   string `json:"provider"`
	ProviderID string `json:"providerId,omitempty"`
	Custom     bool   `json:"custom,omitempty"`
}

func dashboardUserModelSelection(providerID, model string) string {
	return dashboardUserModelPrefix + strings.TrimSpace(providerID) + ":" + strings.TrimSpace(model)
}

func dashboardParseUserModelSelection(value string) (providerID, model string, ok bool) {
	value = strings.TrimSpace(value)
	if !strings.HasPrefix(value, dashboardUserModelPrefix) {
		return "", "", false
	}
	parts := strings.SplitN(strings.TrimPrefix(value, dashboardUserModelPrefix), ":", 2)
	if len(parts) != 2 {
		return "", "", false
	}
	providerID, model = strings.TrimSpace(parts[0]), strings.TrimSpace(parts[1])
	if len(providerID) < 16 || len(providerID) > 64 || !dashboardModelIDSafe(model) {
		return "", "", false
	}
	for _, ch := range providerID {
		if (ch >= 'a' && ch <= 'z') || (ch >= 'A' && ch <= 'Z') || (ch >= '0' && ch <= '9') || ch == '-' || ch == '_' {
			continue
		}
		return "", "", false
	}
	return providerID, model, true
}

func dashboardProviderModelsContain(provider cloud.AIProvider, model string) bool {
	for _, candidate := range provider.Models {
		if candidate == model {
			return true
		}
	}
	return false
}

func dashboardProviderNetworkSafe(ctx context.Context, baseURL string) error {
	normalized, err := cloud.NormalizeAIProviderBaseURL(baseURL)
	if err != nil {
		return err
	}
	parsed, err := url.Parse(normalized)
	if err != nil {
		return err
	}
	host := parsed.Hostname()
	if ip := net.ParseIP(host); ip != nil {
		if !dashboardPublicProviderIP(ip) {
			return errors.New("provider resolves to a private or reserved network")
		}
		return nil
	}
	lookupCtx, cancel := context.WithTimeout(ctx, 4*time.Second)
	defer cancel()
	addresses, err := net.DefaultResolver.LookupIPAddr(lookupCtx, host)
	if err != nil || len(addresses) == 0 {
		return errors.New("provider hostname could not be resolved")
	}
	for _, address := range addresses {
		if !dashboardPublicProviderIP(address.IP) {
			return errors.New("provider hostname resolves to a private or reserved network")
		}
	}
	return nil
}

func dashboardPublicProviderIP(ip net.IP) bool {
	if ip == nil || ip.IsLoopback() || ip.IsPrivate() || ip.IsUnspecified() || ip.IsMulticast() || ip.IsLinkLocalUnicast() || ip.IsLinkLocalMulticast() {
		return false
	}
	if ip4 := ip.To4(); ip4 != nil {
		if ip4[0] == 100 && ip4[1] >= 64 && ip4[1] <= 127 {
			return false
		}
		if (ip4[0] == 192 && ip4[1] == 0 && ip4[2] == 2) || (ip4[0] == 198 && ip4[1] == 51 && ip4[2] == 100) || (ip4[0] == 203 && ip4[1] == 0 && ip4[2] == 113) {
			return false
		}
		return ip4[0] < 224
	}
	return !(len(ip) == net.IPv6len && ip[0] == 0x20 && ip[1] == 0x01 && ip[2] == 0x0d && ip[3] == 0xb8)
}

func dashboardAIProviderError(w http.ResponseWriter, err error) {
	status := http.StatusBadRequest
	message := "Could not update AI provider."
	switch {
	case errors.Is(err, pgx.ErrNoRows):
		status, message = http.StatusNotFound, "AI provider not found."
	case strings.Contains(strings.ToLower(err.Error()), "encryption") || strings.Contains(strings.ToLower(err.Error()), "server key"):
		status, message = http.StatusServiceUnavailable, "Secure provider storage is not configured on this deployment."
	case strings.Contains(strings.ToLower(err.Error()), "https"), strings.Contains(strings.ToLower(err.Error()), "private"), strings.Contains(strings.ToLower(err.Error()), "local host"):
		message = err.Error()
	case strings.Contains(strings.ToLower(err.Error()), "required"), strings.Contains(strings.ToLower(err.Error()), "unsupported"):
		message = err.Error()
	}
	webutil.JSON(w, status, map[string]string{"error": message})
}

func (s *Server) dashboardAIProvidersAPI(w http.ResponseWriter, r *http.Request) {
	identity, ok := s.authenticatedAPIIdentity(w, r)
	if !ok {
		return
	}
	switch r.Method {
	case http.MethodGet:
		providers, err := s.Store.ListAIProviders(r.Context(), identity.User.ID)
		if err != nil {
			dashboardAIProviderError(w, err)
			return
		}
		webutil.JSON(w, http.StatusOK, map[string]any{"providers": providers})
	case http.MethodPost:
		var input dashboardAIProviderMutation
		if err := webutil.DecodeJSON(r, 128<<10, &input); err != nil {
			webutil.JSON(w, http.StatusBadRequest, map[string]string{"error": "Invalid provider settings."})
			return
		}
		if err := dashboardProviderNetworkSafe(r.Context(), input.BaseURL); err != nil {
			dashboardAIProviderError(w, err)
			return
		}
		provider, err := s.Store.CreateAIProvider(r.Context(), identity.User.ID, input.Name, input.BaseURL, input.Protocol, input.APIKey, input.Models, input.ModelLabels)
		if err != nil {
			dashboardAIProviderError(w, err)
			return
		}
		webutil.JSON(w, http.StatusCreated, map[string]any{"provider": provider})
	default:
		webutil.JSON(w, http.StatusMethodNotAllowed, map[string]string{"error": "method_not_allowed"})
	}
}

func (s *Server) dashboardAIProviderAPI(w http.ResponseWriter, r *http.Request) {
	identity, ok := s.authenticatedAPIIdentity(w, r)
	if !ok {
		return
	}
	providerID := strings.TrimSpace(r.PathValue("id"))
	if providerID == "" {
		webutil.JSON(w, http.StatusBadRequest, map[string]string{"error": "Invalid provider id."})
		return
	}
	switch r.Method {
	case http.MethodPatch:
		provider, err := s.Store.GetAIProvider(r.Context(), identity.User.ID, providerID)
		if err != nil {
			dashboardAIProviderError(w, err)
			return
		}
		var input dashboardAIProviderMutation
		if err := webutil.DecodeJSON(r, 128<<10, &input); err != nil {
			webutil.JSON(w, http.StatusBadRequest, map[string]string{"error": "Invalid provider settings."})
			return
		}
		if strings.TrimSpace(input.Name) != "" {
			provider.Name = input.Name
		}
		if strings.TrimSpace(input.BaseURL) != "" {
			provider.BaseURL = input.BaseURL
		}
		if strings.TrimSpace(input.Protocol) != "" {
			provider.Protocol = input.Protocol
		}
		if input.Models != nil {
			provider.Models = input.Models
		}
		if input.ModelLabels != nil {
			provider.ModelLabels = input.ModelLabels
		}
		if input.Enabled != nil {
			provider.Enabled = *input.Enabled
		}
		if err := dashboardProviderNetworkSafe(r.Context(), provider.BaseURL); err != nil {
			dashboardAIProviderError(w, err)
			return
		}
		if err := s.Store.UpdateAIProvider(r.Context(), identity.User.ID, providerID, provider.Name, provider.BaseURL, provider.Protocol, provider.Models, provider.ModelLabels, provider.Enabled); err != nil {
			dashboardAIProviderError(w, err)
			return
		}
		if strings.TrimSpace(input.APIKey) != "" {
			if err := s.Store.RotateAIProviderCredential(r.Context(), identity.User.ID, providerID, input.APIKey); err != nil {
				dashboardAIProviderError(w, err)
				return
			}
		}
		updated, err := s.Store.GetAIProvider(r.Context(), identity.User.ID, providerID)
		if err != nil {
			dashboardAIProviderError(w, err)
			return
		}
		webutil.JSON(w, http.StatusOK, map[string]any{"provider": updated})
	case http.MethodDelete:
		if err := s.Store.DeleteAIProvider(r.Context(), identity.User.ID, providerID); err != nil {
			dashboardAIProviderError(w, err)
			return
		}
		w.WriteHeader(http.StatusNoContent)
	default:
		webutil.JSON(w, http.StatusMethodNotAllowed, map[string]string{"error": "method_not_allowed"})
	}
}

func dashboardTestProviderConnection(ctx context.Context, provider cloud.AIProvider, credential []byte) error {
	if err := dashboardProviderNetworkSafe(ctx, provider.BaseURL); err != nil {
		return err
	}
	if len(provider.Models) == 0 {
		return errors.New("provider has no configured model")
	}
	protocol, ok := dashboardProtocolForUserProvider(provider.Protocol)
	if !ok {
		return errors.New("unsupported provider protocol")
	}
	_, _, err := callLLMWithToolsProtocol(
		protocol,
		provider.BaseURL,
		string(credential),
		provider.Models[0],
		[]map[string]any{{"role": "user", "content": "Reply with OK."}},
		nil,
	)
	return err
}

func (s *Server) dashboardAIProviderTestAPI(w http.ResponseWriter, r *http.Request) {
	identity, ok := s.authenticatedAPIIdentity(w, r)
	if !ok {
		return
	}
	if allowed, _, retry, _ := s.Store.RateLimit(r.Context(), "dashboard-ai-provider-test", identity.User.ID, 10, 60); !allowed {
		w.Header().Set("Retry-After", fmt.Sprintf("%d", retry))
		webutil.JSON(w, http.StatusTooManyRequests, map[string]any{"error": "rate_limited", "retry_after": retry})
		return
	}
	providerID := strings.TrimSpace(r.PathValue("id"))
	provider, err := s.Store.GetAIProvider(r.Context(), identity.User.ID, providerID)
	if err != nil {
		dashboardAIProviderError(w, err)
		return
	}
	credential, err := s.Store.MaterializeAIProviderCredential(r.Context(), identity.User.ID, providerID)
	if err != nil {
		dashboardAIProviderError(w, err)
		return
	}
	defer cloud.ZeroAIProviderCredential(credential)
	probeErr := dashboardTestProviderConnection(r.Context(), provider, credential)
	if probeErr != nil {
		_ = s.Store.UpdateAIProviderProbe(r.Context(), identity.User.ID, providerID, "error", probeErr.Error(), nil)
		webutil.JSON(w, http.StatusBadGateway, map[string]any{"ok": false, "error": "Could not verify this provider. Your saved key was not exposed."})
		return
	}
	message := "Connected · provider verified"
	_ = s.Store.UpdateAIProviderProbe(r.Context(), identity.User.ID, providerID, "ok", message, nil)
	updated, _ := s.Store.GetAIProvider(r.Context(), identity.User.ID, providerID)
	webutil.JSON(w, http.StatusOK, map[string]any{"ok": true, "provider": updated})
}

func dashboardMergeModelSelections(groups ...[]string) []string {
	seen := map[string]bool{}
	out := []string{}
	for _, group := range groups {
		for _, selection := range group {
			selection = strings.TrimSpace(selection)
			_, _, userOwned := dashboardParseUserModelSelection(selection)
			if (!userOwned && !dashboardModelIDSafe(selection)) || seen[selection] {
				continue
			}
			seen[selection] = true
			out = append(out, selection)
		}
	}
	return out
}

func dashboardUserModelOptions(ctx context.Context, s *Server, userID string) ([]string, []dashboardModelOption) {
	if s == nil || s.Store == nil {
		return nil, nil
	}
	providers, err := s.Store.ListAIProviders(ctx, userID)
	if err != nil {
		return nil, nil
	}
	ids := []string{}
	options := []dashboardModelOption{}
	for _, provider := range providers {
		if !provider.Enabled || !provider.HasCredential {
			continue
		}
		for _, model := range provider.Models {
			selection := dashboardUserModelSelection(provider.ID, model)
			if len(selection) > 240 {
				continue
			}
			label := strings.TrimSpace(provider.ModelLabels[model])
			if label == "" {
				label = model
			}
			ids = append(ids, selection)
			options = append(options, dashboardModelOption{ID: selection, Label: label, Provider: provider.Name, ProviderID: provider.ID, Custom: true})
		}
	}
	return ids, options
}

func dashboardProtocolForUserProvider(value string) (dashboardLLMProtocol, bool) {
	switch strings.ToLower(strings.TrimSpace(value)) {
	case cloud.AIProviderProtocolOpenAICompatible, cloud.AIProviderProtocolChatCompletions:
		return dashboardProtocolChatCompletions, true
	case cloud.AIProviderProtocolResponses:
		return dashboardProtocolResponses, true
	case cloud.AIProviderProtocolAnthropicMessages:
		return dashboardProtocolAnthropicMessages, true
	default:
		return dashboardProtocolUnsupported, false
	}
}

type dashboardUserProviderRoute struct {
	Selection string
	Targets   []dashboardLLMTarget
}

type dashboardUserProviderRouteKey struct{}

func dashboardWithUserProviderRoutes(r *http.Request, selection string, targets []dashboardLLMTarget) *http.Request {
	return r.WithContext(context.WithValue(r.Context(), dashboardUserProviderRouteKey{}, dashboardUserProviderRoute{Selection: selection, Targets: targets}))
}

func dashboardUserProviderRoutesFromContext(ctx context.Context, selection string) ([]dashboardLLMTarget, bool) {
	state, ok := ctx.Value(dashboardUserProviderRouteKey{}).(dashboardUserProviderRoute)
	if !ok || state.Selection != selection || len(state.Targets) == 0 {
		return nil, false
	}
	return state.Targets, true
}

func (s *Server) dashboardPrepareUserProviderRoute(r *http.Request, userID, selection string) (*http.Request, func(), error) {
	selection = dashboardNormalizeModelSelection(selection)
	credentials := [][]byte{}
	cleanup := func() {
		for _, credential := range credentials {
			cloud.ZeroAIProviderCredential(credential)
		}
	}
	prepare := func(provider cloud.AIProvider, requestedModels []string) ([]dashboardLLMTarget, error) {
		if !provider.Enabled || !provider.HasCredential {
			return nil, nil
		}
		protocol, ok := dashboardProtocolForUserProvider(provider.Protocol)
		if !ok {
			return nil, errors.New("selected provider protocol is unavailable")
		}
		if err := dashboardProviderNetworkSafe(r.Context(), provider.BaseURL); err != nil {
			return nil, err
		}
		credential, err := s.Store.MaterializeAIProviderCredential(r.Context(), userID, provider.ID)
		if err != nil {
			return nil, err
		}
		credentials = append(credentials, credential)
		targets := make([]dashboardLLMTarget, 0, len(requestedModels))
		for _, model := range requestedModels {
			if !dashboardProviderModelsContain(provider, model) {
				continue
			}
			targets = append(targets, dashboardLLMTarget{ID: dashboardUserModelSelection(provider.ID, model), BaseURL: provider.BaseURL, APIKeyBytes: credential, Model: model, Protocol: protocol, Vision: dashboardModelSupportsVision(model)})
		}
		return targets, nil
	}
	if selection == dashboardModelAuto {
		providers, err := s.Store.ListAIProviders(r.Context(), userID)
		if err != nil {
			return r, cleanup, err
		}
		targets := []dashboardLLMTarget{}
		for _, provider := range providers {
			providerTargets, err := prepare(provider, provider.Models)
			if err != nil {
				cleanup()
				return r, func() {}, err
			}
			targets = append(targets, providerTargets...)
		}
		if len(targets) == 0 {
			cleanup()
			return r, func() {}, errors.New("no user AI model is configured")
		}
		return dashboardWithUserProviderRoutes(r, selection, targets), cleanup, nil
	}
	providerID, model, ok := dashboardParseUserModelSelection(selection)
	if !ok {
		return r, func() {}, errors.New("selected AI model is not configured by this user")
	}
	provider, err := s.Store.GetAIProvider(r.Context(), userID, providerID)
	if err != nil {
		return r, func() {}, err
	}
	targets, err := prepare(provider, []string{model})
	if err != nil {
		cleanup()
		return r, func() {}, err
	}
	if len(targets) == 0 {
		cleanup()
		return r, func() {}, errors.New("selected provider model is unavailable")
	}
	return dashboardWithUserProviderRoutes(r, selection, targets), cleanup, nil
}
