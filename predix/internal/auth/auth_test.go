package auth_test

import (
	"net/http"
	"net/http/httptest"
	"net/url"
	"testing"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/golang-jwt/jwt/v5"
	"github.com/google/uuid"
	"github.com/lowkeylab/bazel-repo/predix/internal/auth"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestMiddlewareCredentials(t *testing.T) {
	const secret = "fixture-only-secret"
	const alice = "49b97840-516f-448f-a175-944c9ef809e0"
	const bob = "d02dfc97-d78b-4f84-b30e-3541fa554db5"
	sign := func(method jwt.SigningMethod, claims jwt.MapClaims, key any) string {
		t.Helper()
		token, err := jwt.NewWithClaims(method, claims).SignedString(key)
		require.NoError(t, err)
		return token
	}
	valid := sign(jwt.SigningMethodHS256, jwt.MapClaims{"uid": alice, "exp": time.Now().Add(time.Hour).Unix()}, []byte(secret))
	other := sign(jwt.SigningMethodHS256, jwt.MapClaims{"uid": bob}, []byte(secret))
	type credentialCase struct {
		name      string
		header    string
		query     string
		identity  string
		rejection string
	}
	cases := []credentialCase{
		{name: "bearer", header: "Bearer " + valid, identity: alice},
		{name: "case insensitive bearer", header: "bEaReR " + valid, identity: alice},
		{name: "query token", query: valid, identity: alice},
		{name: "empty header falls back", header: "", query: valid, identity: alice},
		{name: "empty bearer falls back", header: "Bearer ", query: valid, identity: alice},
		{name: "bare scheme falls back", header: "Bearer", query: valid, identity: alice},
		{name: "unsupported scheme falls back", header: "Basic credentials", query: valid, identity: alice},
		{name: "bearer identity takes precedence", header: "Bearer " + valid, query: other, identity: alice},
		{name: "valid bearer ignores invalid query", header: "Bearer " + valid, query: "invalid", identity: alice},
		{name: "expiration remains optional", header: "Bearer " + other, identity: bob},
		{name: "nil UUID remains accepted", query: sign(jwt.SigningMethodHS256, jwt.MapClaims{"uid": uuid.Nil.String()}, []byte(secret)), identity: uuid.Nil.String()},
		{name: "missing credentials", rejection: "authorization required"},
		{name: "unsupported scheme without query", header: "Basic credentials", rejection: "authorization required"},
		{name: "bare scheme without query", header: "Bearer", rejection: "authorization required"},
		{name: "empty bearer without query", header: "Bearer ", rejection: "authorization required"},
		{name: "malformed token", header: "Bearer not.a.jwt", rejection: "invalid token"},
		{name: "invalid query", query: "invalid", rejection: "invalid token"},
		{name: "invalid bearer cannot fall back", header: "Bearer invalid", query: valid, rejection: "invalid token"},
		{name: "expired token", header: "Bearer " + sign(jwt.SigningMethodHS256, jwt.MapClaims{"uid": alice, "exp": 946684800}, []byte(secret)), rejection: "invalid token"},
		{name: "wrong signature", header: "Bearer " + sign(jwt.SigningMethodHS256, jwt.MapClaims{"uid": alice}, []byte("other-fixture-key")), rejection: "invalid token"},
		{name: "disallowed HMAC algorithm", header: "Bearer " + sign(jwt.SigningMethodHS384, jwt.MapClaims{"uid": alice}, []byte(secret)), rejection: "invalid token"},
		{name: "unsigned token", header: "Bearer " + sign(jwt.SigningMethodNone, jwt.MapClaims{"uid": alice}, jwt.UnsafeAllowNoneSignatureType), rejection: "invalid token"},
	}
	for _, invalid := range []struct {
		name   string
		claims jwt.MapClaims
	}{
		{"missing", jwt.MapClaims{}},
		{"null", jwt.MapClaims{"uid": nil}},
		{"number", jwt.MapClaims{"uid": 42}},
		{"boolean", jwt.MapClaims{"uid": true}},
		{"object", jwt.MapClaims{"uid": map[string]string{"id": alice}}},
		{"array", jwt.MapClaims{"uid": []string{alice}}},
		{"empty", jwt.MapClaims{"uid": ""}},
		{"malformed UUID", jwt.MapClaims{"uid": "not-a-uuid"}},
	} {
		cases = append(cases, credentialCase{name: invalid.name + " identity claim", header: "Bearer " + sign(jwt.SigningMethodHS256, invalid.claims, []byte(secret)), rejection: "invalid token"})
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			router := gin.New()
			reached := false
			router.GET("/protected", auth.NewManager(secret, time.Hour).Middleware(), func(c *gin.Context) {
				reached = true
				id, ok := auth.UserIDFromContext(c)
				assert.True(t, ok, "authenticated identity must be available")
				c.String(http.StatusOK, uuid.UUID(id).String())
			})
			req := httptest.NewRequest(http.MethodGet, "/protected?token="+url.QueryEscape(tc.query), nil)
			req.Header.Set("Authorization", tc.header)
			req.Header.Set(auth.TestUserIDHeader, bob)
			response := httptest.NewRecorder()
			router.ServeHTTP(response, req)
			if tc.rejection != "" {
				assert.Equal(t, http.StatusUnauthorized, response.Code)
				assert.JSONEq(t, `{"error":"`+tc.rejection+`"}`, response.Body.String())
				assert.False(t, reached, "rejected requests must not execute protected work")
			} else {
				assert.Equal(t, http.StatusOK, response.Code)
				assert.Equal(t, tc.identity, response.Body.String())
				assert.True(t, reached)
			}
		})
	}
}
