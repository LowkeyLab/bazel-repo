package http

import (
	"net/http"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/google/uuid"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest"
	contestservice "github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/service"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
)

type Handler struct {
	svc *contestservice.Service
}

func NewHandler(svc *contestservice.Service) *Handler {
	return &Handler{svc: svc}
}

func (h *Handler) RegisterRoutes(r *gin.Engine) {
	r.POST("/contests", h.createContest)
	r.POST("/contests/:id/predict", h.predict)
	r.POST("/contests/:id/resolve", h.resolveContest)
}

type createContestRequest struct {
	CircleID  string   `json:"circle_id"`
	CreatorID string   `json:"creator_id"`
	Question  string   `json:"question"`
	Options   []string `json:"options"`
	ExpiresAt string   `json:"expires_at"` // RFC3339
}

type createContestResponse struct {
	ID       string `json:"id"`
	Question string `json:"question"`
}

func (h *Handler) createContest(c *gin.Context) {
	var req createContestRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid request body"})
		return
	}

	expiresAt, err := time.Parse(time.RFC3339, req.ExpiresAt)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid expires_at format (use RFC3339)"})
		return
	}

	circleID, err := uuid.Parse(req.CircleID)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid circle_id"})
		return
	}

	creatorID, err := uuid.Parse(req.CreatorID)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid creator_id"})
		return
	}

	contestObj, err := h.svc.CreateContest(c.Request.Context(), circle.ID(circleID), user.ID(creatorID), req.Question, req.Options, expiresAt)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}

	c.JSON(http.StatusOK, createContestResponse{
		ID:       contestObj.ID.String(),
		Question: contestObj.Question,
	})
}

type predictRequest struct {
	UserID   string `json:"user_id"`
	OptionID int    `json:"option_id"`
	Clout    int    `json:"clout"`
}

func (h *Handler) predict(c *gin.Context) {
	contestIDStr := c.Param("id")
	if contestIDStr == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing contest id"})
		return
	}

	contestID, err := uuid.Parse(contestIDStr)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid contest id"})
		return
	}

	var req predictRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid request body"})
		return
	}

	userID, err := uuid.Parse(req.UserID)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid user id"})
		return
	}

	err = h.svc.Predict(c.Request.Context(), contest.ID(contestID), user.ID(userID), req.OptionID, req.Clout)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}

	c.Status(http.StatusCreated)
}

type resolveContestRequest struct {
	WinningOptionID int `json:"winning_option_id"`
}

func (h *Handler) resolveContest(c *gin.Context) {
	contestIDStr := c.Param("id")
	if contestIDStr == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing contest id"})
		return
	}

	contestID, err := uuid.Parse(contestIDStr)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid contest id"})
		return
	}

	var req resolveContestRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid request body"})
		return
	}

	err = h.svc.ResolveContest(c.Request.Context(), contest.ID(contestID), req.WinningOptionID)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}

	c.Status(http.StatusOK)
}
