package rest

import (
	"net/http"

	sloggin "github.com/gin-contrib/slog"
	"github.com/gin-gonic/gin"
	"github.com/google/uuid"
	"github.com/lowkeylab/bazel-repo/predix/internal/auth"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user/service"
)

// Handler wires user authentication endpoints.
type Handler struct {
	svc    *service.Service
	tokens *auth.Manager
}

// NewHandler constructs a user REST handler.
func NewHandler(svc *service.Service, tokens *auth.Manager) *Handler {
	return &Handler{svc: svc, tokens: tokens}
}

// RegisterRoutes registers user-related routes on the provided router.
func (h *Handler) RegisterRoutes(r gin.IRoutes) {
	r.POST("/register", h.register)
	r.POST("/login", h.login)
}

type loginRequest struct {
	Username string `json:"username"`
	Password string `json:"password"`
}

type userResponse struct {
	ID       string `json:"id"`
	Username string `json:"username"`
	Role     string `json:"role"`
}

type loginResponse struct {
	Token string       `json:"token"`
	User  userResponse `json:"user"`
}

func (h *Handler) register(c *gin.Context) {
	var req loginRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		sloggin.Get(c).Warn("invalid register request body", "error", err)
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid request body"})
		return
	}

	u, err := h.svc.Register(c.Request.Context(), req.Username, req.Password)
	if err != nil {
		sloggin.Get(c).Warn("registration failed", "username", req.Username, "error", err)
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	token, err := h.tokens.GenerateToken(u)
	if err != nil {
		sloggin.Get(c).Error("failed to generate token", "user_id", u.ID, "error", err)
		c.JSON(http.StatusInternalServerError, gin.H{"error": "failed to issue token"})
		return
	}

	sloggin.Get(c).Info("user registered successfully", "user_id", u.ID, "username", u.Username)
	c.JSON(http.StatusCreated, loginResponse{
		Token: token,
		User:  toUserResponse(u),
	})
}

func (h *Handler) login(c *gin.Context) {
	var req loginRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		sloggin.Get(c).Warn("invalid login request body", "error", err)
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid request body"})
		return
	}

	u, err := h.svc.Login(c.Request.Context(), req.Username, req.Password)
	if err != nil {
		sloggin.Get(c).Warn("login failed", "username", req.Username, "error", err)
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	token, err := h.tokens.GenerateToken(u)
	if err != nil {
		sloggin.Get(c).Error("failed to generate token", "user_id", u.ID, "error", err)
		c.JSON(http.StatusInternalServerError, gin.H{"error": "failed to issue token"})
		return
	}

	sloggin.Get(c).Info("user logged in successfully", "user_id", u.ID, "username", u.Username)
	c.JSON(http.StatusOK, loginResponse{
		Token: token,
		User:  toUserResponse(u),
	})
}

func toUserResponse(u *user.User) userResponse {
	return userResponse{
		ID:       uuid.UUID(u.ID).String(),
		Username: u.Username,
		Role:     string(u.Role),
	}
}
