package healthcheck

import "github.com/gin-gonic/gin"

func healthcheck(c *gin.Context) {
	c.String(200, "OK")
}

// RegisterRoutes wires the healthcheck endpoint into the provided router.
func RegisterRoutes(r gin.IRoutes) {
	r.GET("/health", healthcheck)
}
