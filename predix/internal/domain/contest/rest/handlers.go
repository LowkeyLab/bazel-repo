package rest

import (
	"errors"
	"log/slog"
	"net/http"
	"sort"
	"strconv"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/jackc/pgx/v5"
	"github.com/lowkeylab/bazel-repo/predix/internal/auth"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
	circleservice "github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/service"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest/service"
)

// Handler wires contest HTTP endpoints.
type Handler struct {
	svc       *service.Service
	circleSvc *circleservice.Service
}

// NewHandler constructs a contest REST handler.
func NewHandler(svc *service.Service, circleSvc *circleservice.Service) *Handler {
	return &Handler{svc: svc, circleSvc: circleSvc}
}

// RegisterRoutes registers contest routes on the provided router.
func (h *Handler) RegisterRoutes(r gin.IRoutes) {
	r.POST("/contests", h.createContest)
	r.GET("/contests/:id", h.getContest)
	r.GET("/contests/:id/payout-breakdown", h.getPayoutBreakdown)
	r.POST("/contests/:id/predictions", h.makePrediction)
	r.POST("/contests/:id/lock", h.lockContest)
	r.POST("/contests/:id/resolve", h.resolveContest)
}

type createContestRequest struct {
	CircleID  int32     `json:"circle_id"`
	Question  string    `json:"question"`
	Options   []string  `json:"options"`
	MinStake  int       `json:"min_stake"`
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
	CircleID       int32                `json:"circle_id"`
	CreatorID      int32                `json:"creator_id"`
	Question       string               `json:"question"`
	Options        []optionResponse     `json:"options"`
	Predictions    []predictionResponse `json:"predictions"`
	Status         string               `json:"status"`
	MinStake       int                  `json:"min_stake"`
	TotalPot       int                  `json:"total_pot"`
	CloutConsumed  int                  `json:"clout_consumed"`
	ResultOptionID *int                 `json:"result_option_id,omitempty"`
	CreatedAt      time.Time            `json:"created_at"`
	ExpiresAt      time.Time            `json:"expires_at"`
}

type makePredictionRequest struct {
	OptionID int `json:"option_id"`
	Clout    int `json:"clout"`
}

type resolveContestRequest struct {
	WinningOptionID int `json:"winning_option_id"`
}

type payoutRecord struct {
	UserID int32 `json:"user_id"`
	Stake  int   `json:"stake"`
	Share  int   `json:"share"`
	Total  int   `json:"total"`
}

type payoutBreakdownResponse struct {
	Winners          []payoutRecord `json:"winners"`
	TotalPot         int            `json:"total_pot"`
	CloutConsumed    int            `json:"clout_consumed"`
	DistributablePot int            `json:"distributable_pot"`
	TotalDistributed int            `json:"total_distributed"`
}

func (h *Handler) createContest(c *gin.Context) {
	var req createContestRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		slog.WarnContext(c.Request.Context(), "invalid create contest request body", "error", err)
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid request body"})
		return
	}

	creatorID, ok := auth.UserIDFromContext(c)
	if !ok {
		slog.WarnContext(c.Request.Context(), "unauthenticated contest creation attempt")
		c.JSON(http.StatusUnauthorized, gin.H{"error": "authentication required"})
		return
	}

	newContest, err := h.svc.CreateContest(
		c.Request.Context(),
		circle.ID(req.CircleID),
		creatorID,
		req.Question,
		req.Options,
		req.ExpiresAt,
		req.MinStake,
	)
	if err != nil {
		slog.WarnContext(c.Request.Context(), "failed to create contest", "creator_id", creatorID, "question", req.Question, "circle_id", req.CircleID, "error", err)
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	slog.InfoContext(c.Request.Context(), "contest created successfully", "contest_id", newContest.ID, "creator_id", creatorID, "question", req.Question, "circle_id", req.CircleID)
	c.JSON(http.StatusCreated, toContestResponse(newContest))
}

func (h *Handler) getContest(c *gin.Context) {
	contestID, ok := parseContestID(c)
	if !ok {
		return
	}

	result, err := h.svc.GetContest(c.Request.Context(), contestID)
	if err != nil {
		if errors.Is(err, pgx.ErrNoRows) {
			slog.DebugContext(c.Request.Context(), "contest not found", "contest_id", contestID)
			c.JSON(http.StatusNotFound, gin.H{"error": "contest not found"})
			return
		}
		slog.ErrorContext(c.Request.Context(), "failed to get contest", "contest_id", contestID, "error", err)
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}

	slog.DebugContext(c.Request.Context(), "contest retrieved", "contest_id", contestID)
	c.JSON(http.StatusOK, toContestResponse(result))
}

func (h *Handler) makePrediction(c *gin.Context) {
	contestID, ok := parseContestID(c)
	if !ok {
		return
	}

	userID, ok := auth.UserIDFromContext(c)
	if !ok {
		slog.WarnContext(c.Request.Context(), "unauthenticated prediction attempt")
		c.JSON(http.StatusUnauthorized, gin.H{"error": "authentication required"})
		return
	}

	var req makePredictionRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		slog.WarnContext(c.Request.Context(), "invalid make prediction request body", "error", err)
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid request body"})
		return
	}

	err := h.circleSvc.Predict(c.Request.Context(), contestID, userID, req.OptionID, req.Clout)
	if err != nil {
		if errors.Is(err, pgx.ErrNoRows) {
			slog.WarnContext(c.Request.Context(), "circle or contest not found", "contest_id", contestID)
			c.JSON(http.StatusNotFound, gin.H{"error": "circle or contest not found"})
			return
		}
		if errors.Is(err, circleservice.ErrUserNotInCircle) {
			slog.WarnContext(c.Request.Context(), "user not in circle", "contest_id", contestID, "user_id", userID)
			c.JSON(http.StatusForbidden, gin.H{"error": err.Error()})
			return
		}
		if errors.Is(err, circleservice.ErrInsufficientClout) {
			slog.WarnContext(c.Request.Context(), "insufficient clout", "contest_id", contestID, "user_id", userID)
			c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
			return
		}
		slog.WarnContext(c.Request.Context(), "failed to make prediction", "contest_id", contestID, "user_id", userID, "option_id", req.OptionID, "error", err)
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	slog.InfoContext(c.Request.Context(), "prediction made successfully", "contest_id", contestID, "user_id", userID, "option_id", req.OptionID)
	c.Status(http.StatusCreated)
}

func (h *Handler) lockContest(c *gin.Context) {
	contestID, ok := parseContestID(c)
	if !ok {
		return
	}

	userID, ok := auth.UserIDFromContext(c)
	if !ok {
		slog.WarnContext(c.Request.Context(), "unauthenticated lock attempt")
		c.JSON(http.StatusUnauthorized, gin.H{"error": "authentication required"})
		return
	}

	err := h.svc.LockContest(c.Request.Context(), contestID, userID)
	if err != nil {
		if errors.Is(err, pgx.ErrNoRows) {
			slog.WarnContext(c.Request.Context(), "contest not found", "contest_id", contestID)
			c.JSON(http.StatusNotFound, gin.H{"error": "contest not found"})
			return
		}
		if errors.Is(err, service.ErrNotContestCreator) {
			slog.WarnContext(c.Request.Context(), "unauthorized contest lock", "contest_id", contestID, "user_id", userID)
			c.JSON(http.StatusForbidden, gin.H{"error": err.Error()})
			return
		}
		slog.WarnContext(c.Request.Context(), "failed to lock contest", "contest_id", contestID, "user_id", userID, "error", err)
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	slog.InfoContext(c.Request.Context(), "contest locked successfully", "contest_id", contestID, "user_id", userID)
	c.Status(http.StatusOK)
}

func (h *Handler) resolveContest(c *gin.Context) {
	contestID, ok := parseContestID(c)
	if !ok {
		return
	}

	userID, ok := auth.UserIDFromContext(c)
	if !ok {
		slog.WarnContext(c.Request.Context(), "unauthenticated resolve attempt")
		c.JSON(http.StatusUnauthorized, gin.H{"error": "authentication required"})
		return
	}

	var req resolveContestRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		slog.WarnContext(c.Request.Context(), "invalid resolve contest request body", "error", err)
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid request body"})
		return
	}

	err := h.circleSvc.ResolveAndDistributeContestClout(c.Request.Context(), contestID, userID, req.WinningOptionID)
	if err != nil {
		if errors.Is(err, pgx.ErrNoRows) {
			slog.WarnContext(c.Request.Context(), "circle or contest not found", "contest_id", contestID)
			c.JSON(http.StatusNotFound, gin.H{"error": "circle or contest not found"})
			return
		}
		if errors.Is(err, service.ErrNotContestCreator) {
			slog.WarnContext(c.Request.Context(), "unauthorized contest resolution", "contest_id", contestID, "user_id", userID)
			c.JSON(http.StatusForbidden, gin.H{"error": err.Error()})
			return
		}
		slog.WarnContext(c.Request.Context(), "failed to resolve contest", "contest_id", contestID, "user_id", userID, "winning_option_id", req.WinningOptionID, "error", err)
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	slog.InfoContext(c.Request.Context(), "contest resolved and clout distributed", "contest_id", contestID, "user_id", userID, "winning_option_id", req.WinningOptionID)
	c.Status(http.StatusOK)
}

func (h *Handler) getPayoutBreakdown(c *gin.Context) {
	contestID, ok := parseContestID(c)
	if !ok {
		return
	}

	// Get the contest to verify it's resolved and get contest details
	cont, err := h.svc.GetContest(c.Request.Context(), contestID)
	if err != nil {
		if errors.Is(err, pgx.ErrNoRows) {
			slog.WarnContext(c.Request.Context(), "contest not found", "contest_id", contestID)
			c.JSON(http.StatusNotFound, gin.H{"error": "contest not found"})
			return
		}
		slog.ErrorContext(c.Request.Context(), "failed to get contest", "contest_id", contestID, "error", err)
		c.JSON(http.StatusInternalServerError, gin.H{"error": "internal error"})
		return
	}

	// Check if contest is resolved
	if cont.Status != contest.StatusResolved {
		slog.WarnContext(c.Request.Context(), "contest not resolved", "contest_id", contestID, "status", cont.Status)
		c.JSON(http.StatusBadRequest, gin.H{"error": "contest must be resolved to view payout breakdown"})
		return
	}

	if cont.ResultOptionID == nil {
		slog.WarnContext(c.Request.Context(), "resolved contest has no winning option", "contest_id", contestID)
		c.JSON(http.StatusBadRequest, gin.H{"error": "resolved contest has no winning option"})
		return
	}

	// Calculate payout breakdown
	totalPot := cont.CalculatePot()
	cloutConsumed := cont.CalculateConsumedClout()
	distributablePot := cont.CalculateRemainingPot()

	// Find all winning predictions and calculate payouts
	var winningPredictions []*contest.Prediction
	var totalWinningClout int

	for i := range cont.Predictions {
		if cont.Predictions[i].OptionID == *cont.ResultOptionID {
			winningPredictions = append(winningPredictions, cont.Predictions[i])
			totalWinningClout += cont.Predictions[i].Clout
		}
	}

	// Build payout records
	payoutRecords := make([]payoutRecord, 0, len(winningPredictions))
	totalDistributed := 0

	for _, pred := range winningPredictions {
		stake := pred.Clout
		share := 0
		if totalWinningClout > 0 {
			share = (stake * distributablePot) / totalWinningClout
		}
		total := stake + share
		totalDistributed += total

		payoutRecords = append(payoutRecords, payoutRecord{
			UserID: int32(pred.UserID),
			Stake:  stake,
			Share:  share,
			Total:  total,
		})
	}

	response := payoutBreakdownResponse{
		Winners:          payoutRecords,
		TotalPot:         totalPot,
		CloutConsumed:    cloutConsumed,
		DistributablePot: distributablePot,
		TotalDistributed: totalDistributed,
	}

	slog.InfoContext(c.Request.Context(), "payout breakdown retrieved", "contest_id", contestID, "winner_count", len(winningPredictions))
	c.JSON(http.StatusOK, response)
}

func parseContestID(c *gin.Context) (contest.ID, bool) {
	rawID := c.Param("id")
	if rawID == "" {
		slog.WarnContext(c.Request.Context(), "missing contest id parameter")
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing contest id"})
		return 0, false
	}

	parsed, err := strconv.ParseInt(rawID, 10, 32)
	if err != nil {
		slog.WarnContext(c.Request.Context(), "invalid contest id format", "id", rawID, "error", err)
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid contest id"})
		return 0, false
	}

	return contest.ID(parsed), true
}

func toContestResponse(cont *contest.Contest) contestResponse {
	options := make([]optionResponse, 0, len(cont.Options))
	for _, opt := range cont.Options {
		options = append(options, optionResponse{ID: opt.ID, Text: opt.Text})
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
		CircleID:       int32(cont.CircleID),
		CreatorID:      int32(cont.CreatorID),
		Question:       cont.Question,
		Options:        options,
		Predictions:    predictions,
		Status:         string(cont.Status),
		MinStake:       cont.MinStake,
		TotalPot:       cont.CalculatePot(),
		CloutConsumed:  cont.CalculateConsumedClout(),
		ResultOptionID: cont.ResultOptionID,
		CreatedAt:      cont.CreatedAt,
		ExpiresAt:      cont.ExpiresAt,
	}
}
