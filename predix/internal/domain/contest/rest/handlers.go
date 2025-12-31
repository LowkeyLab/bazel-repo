package rest

import (
	"errors"
	"net/http"
	"sort"
	"strconv"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/jackc/pgx/v5"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/service"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
)

// Handler wires contest HTTP endpoints.
type Handler struct {
	svc *service.Service
}

// NewHandler constructs a contest REST handler.
func NewHandler(svc *service.Service) *Handler {
	return &Handler{svc: svc}
}

// RegisterRoutes registers contest routes on the provided router.
func (h *Handler) RegisterRoutes(r *gin.Engine) {
	r.POST("/contests", h.createContest)
	r.POST("/contests/:id/predictions", h.makePrediction)
	r.POST("/contests/:id/resolve", h.resolveContest)
	r.GET("/contests/:id", h.getContest)
}

type createContestRequest struct {
	CircleIDs []int32   `json:"circle_ids"`
	CreatorID int32     `json:"creator_id"`
	Question  string    `json:"question"`
	Options   []string  `json:"options"`
	ExpiresAt time.Time `json:"expires_at"`
}

type optionResponse struct {
	ID   int    `json:"id"`
	Text string `json:"text"`
}

type predictionResponse struct {
	UserID    int32     `json:"user_id"`
	OptionID  int       `json:"option_id"`
	Clout     int       `json:"clout"`
	Timestamp time.Time `json:"timestamp"`
}

type contestResponse struct {
	ID             int32                `json:"id"`
	CircleIDs      []int32              `json:"circle_ids"`
	CreatorID      int32                `json:"creator_id"`
	Question       string               `json:"question"`
	Options        []optionResponse     `json:"options"`
	Predictions    []predictionResponse `json:"predictions"`
	Status         string               `json:"status"`
	ResultOptionID *int                 `json:"result_option_id,omitempty"`
	CreatedAt      time.Time            `json:"created_at"`
	ExpiresAt      time.Time            `json:"expires_at"`
}

func (h *Handler) createContest(c *gin.Context) {
	var req createContestRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid request body"})
		return
	}

	circleIDs := make([]circle.ID, len(req.CircleIDs))
	for i, id := range req.CircleIDs {
		circleIDs[i] = circle.ID(id)
	}

	newContest, err := h.svc.CreateContest(
		c.Request.Context(),
		circleIDs,
		user.ID(req.CreatorID),
		req.Question,
		req.Options,
		req.ExpiresAt,
	)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	c.JSON(http.StatusCreated, toContestResponse(newContest))
}

type makePredictionRequest struct {
	UserID   int32 `json:"user_id"`
	OptionID int   `json:"option_id"`
	Clout    int   `json:"clout"`
}

func (h *Handler) makePrediction(c *gin.Context) {
	contestID, ok := parseContestID(c)
	if !ok {
		return
	}

	var req makePredictionRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid request body"})
		return
	}

	err := h.svc.Predict(c.Request.Context(), contestID, user.ID(req.UserID), req.OptionID, req.Clout)
	if err != nil {
		if errors.Is(err, pgx.ErrNoRows) {
			c.JSON(http.StatusNotFound, gin.H{"error": "contest not found"})
			return
		}
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	c.Status(http.StatusCreated)
}

type resolveContestRequest struct {
	WinningOptionID int `json:"winning_option_id"`
}

func (h *Handler) resolveContest(c *gin.Context) {
	contestID, ok := parseContestID(c)
	if !ok {
		return
	}

	var req resolveContestRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid request body"})
		return
	}

	err := h.svc.ResolveContest(c.Request.Context(), contestID, req.WinningOptionID)
	if err != nil {
		if errors.Is(err, pgx.ErrNoRows) {
			c.JSON(http.StatusNotFound, gin.H{"error": "contest not found"})
			return
		}
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	c.Status(http.StatusOK)
}

func (h *Handler) getContest(c *gin.Context) {
	contestID, ok := parseContestID(c)
	if !ok {
		return
	}

	result, err := h.svc.GetContest(c.Request.Context(), contestID)
	if err != nil {
		if errors.Is(err, pgx.ErrNoRows) {
			c.JSON(http.StatusNotFound, gin.H{"error": "contest not found"})
			return
		}
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}

	c.JSON(http.StatusOK, toContestResponse(result))
}

func parseContestID(c *gin.Context) (contest.ID, bool) {
	rawID := c.Param("id")
	if rawID == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing contest id"})
		return 0, false
	}

	parsed, err := strconv.ParseInt(rawID, 10, 32)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid contest id"})
		return 0, false
	}

	return contest.ID(parsed), true
}

func toContestResponse(cont *contest.Contest) contestResponse {
	circleIDs := make([]int32, len(cont.CircleIDs))
	for i, id := range cont.CircleIDs {
		circleIDs[i] = int32(id)
	}

	options := make([]optionResponse, 0, len(cont.Options))
	for _, opt := range cont.Options {
		options = append(options, optionResponse{
			ID:   opt.ID,
			Text: opt.Text,
		})
	}
	sort.Slice(options, func(i, j int) bool {
		return options[i].ID < options[j].ID
	})

	predictions := make([]predictionResponse, len(cont.Predictions))
	for i, pred := range cont.Predictions {
		predictions[i] = predictionResponse{
			UserID:    int32(pred.UserID),
			OptionID:  pred.OptionID,
			Clout:     pred.Clout,
			Timestamp: pred.Timestamp,
		}
	}

	return contestResponse{
		ID:             int32(cont.ID),
		CircleIDs:      circleIDs,
		CreatorID:      int32(cont.CreatorID),
		Question:       cont.Question,
		Options:        options,
		Predictions:    predictions,
		Status:         string(cont.Status),
		ResultOptionID: cont.ResultOptionID,
		CreatedAt:      cont.CreatedAt,
		ExpiresAt:      cont.ExpiresAt,
	}
}
