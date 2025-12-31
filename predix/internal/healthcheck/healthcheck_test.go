package healthcheck

import (
	"net/http/httptest"
	"testing"

	"github.com/gin-gonic/gin"
)

func TestHealthCheck(t *testing.T) {
	gin.SetMode(gin.TestMode)

	r := gin.New()
	RegisterRoutes(r)

	resp := httptest.NewRecorder()
	req := httptest.NewRequest("GET", "/health", nil)
	r.ServeHTTP(resp, req)

	if resp.Code != 200 {
		t.Errorf("Expected status code 200, got %d", resp.Code)
	}
}
