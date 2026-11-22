package cloudserver

import (
	"context"
	"fmt"
	"html"
	"log/slog"
	"net/url"
	"os"
	"strings"
	"time"

	"github.com/0xmarkhydra/codelocal/internal/cloud"
	"github.com/0xmarkhydra/codelocal/internal/mailer"
)

const forumEmailPollInterval = 5 * time.Second

func forumEmailTopicURL(topicID string) string {
	base := strings.TrimRight(strings.TrimSpace(os.Getenv("PUBLIC_BASE_URL")), "/")
	if base == "" {
		base = "https://codelocal.cloud"
	}
	return base + "/dashboard/forums/" + url.PathEscape(strings.TrimSpace(topicID))
}

func forumEmailSubjectPart(value string) string {
	value = strings.Join(strings.Fields(value), " ")
	if len(value) > 120 {
		value = value[:117] + "..."
	}
	return value
}

func forumEmailSummary(value string, limit int) string {
	value = strings.TrimSpace(value)
	if limit < 1 || len(value) <= limit {
		return value
	}
	return strings.TrimSpace(value[:limit]) + "..."
}

func forumEmailHTML(title, intro, body, topicURL string) string {
	escapeWithBreaks := func(value string) string {
		return strings.ReplaceAll(html.EscapeString(strings.TrimSpace(value)), "\n", "<br>")
	}
	return `<div style="font-family:-apple-system,BlinkMacSystemFont,Segoe UI,sans-serif;max-width:640px;margin:auto;padding:32px 24px;color:#111">` +
		`<h2 style="margin:0 0 12px;font-size:22px">` + html.EscapeString(title) + `</h2>` +
		`<p style="margin:0 0 18px;color:#555;line-height:1.6">` + escapeWithBreaks(intro) + `</p>` +
		`<div style="background:#f5f5f7;border-radius:12px;padding:16px;line-height:1.65;margin:0 0 22px">` + escapeWithBreaks(body) + `</div>` +
		`<a href="` + html.EscapeString(topicURL) + `" style="display:inline-block;background:#111;color:#fff;text-decoration:none;padding:11px 16px;border-radius:10px;font-weight:600">Open Forums</a>` +
		`<p style="margin:22px 0 0;color:#888;font-size:12px">CodeLocal Forums notification</p>` +
		`</div>`
}

func forumAdminTopicCreatedEmail(topic cloud.ForumTopic) (string, string, string) {
	topicURL := forumEmailTopicURL(topic.ID)
	subject := "[Forums] New " + strings.ToLower(strings.TrimSpace(topic.Kind)) + ": " + forumEmailSubjectPart(topic.Title)
	body := fmt.Sprintf("Type: %s\nAuthor: %s\nStatus: %s\n\n%s", topic.Kind, topic.AuthorEmail, topic.Status, forumEmailSummary(topic.Body, 1600))
	text := "A new topic was posted on CodeLocal Forums.\n\n" + body + "\n\nOpen: " + topicURL
	htmlBody := forumEmailHTML("New forum topic", "A new topic was posted on CodeLocal Forums.", body, topicURL)
	return subject, text, htmlBody
}

func forumAdminCommentCreatedEmail(topic cloud.ForumTopic, comment cloud.ForumComment) (string, string, string) {
	topicURL := forumEmailTopicURL(topic.ID)
	subject := "[Forums] New reply: " + forumEmailSubjectPart(topic.Title)
	body := fmt.Sprintf("Topic: %s\nReply by: %s\n\n%s", topic.Title, comment.AuthorEmail, forumEmailSummary(comment.Body, 1600))
	text := "A new reply was posted on CodeLocal Forums.\n\n" + body + "\n\nOpen: " + topicURL
	htmlBody := forumEmailHTML("New forum reply", "A new reply was posted on CodeLocal Forums.", body, topicURL)
	return subject, text, htmlBody
}

func forumTopicOwnerCommentEmail(topic cloud.ForumTopic, comment cloud.ForumComment) (string, string, string) {
	topicURL := forumEmailTopicURL(topic.ID)
	replier := "Community member"
	if cloud.IsAdminEmail(comment.AuthorEmail) {
		replier = "CodeLocal Team"
	}
	subject := "Your CodeLocal Forums topic has a new reply: " + forumEmailSubjectPart(topic.Title)
	body := fmt.Sprintf("Topic: %s\nReply from: %s\n\n%s", topic.Title, replier, forumEmailSummary(comment.Body, 1600))
	text := "Your topic on CodeLocal Forums has a new reply.\n\n" + body + "\n\nOpen: " + topicURL
	htmlBody := forumEmailHTML("Your topic has a new reply", "Someone replied to your topic on CodeLocal Forums.", body, topicURL)
	return subject, text, htmlBody
}

func forumTopicOwnerStatusEmail(topic cloud.ForumTopic) (string, string, string) {
	topicURL := forumEmailTopicURL(topic.ID)
	subject := "CodeLocal updated your forum topic: " + forumEmailSubjectPart(topic.Title)
	if topic.Status == "resolved" {
		subject = "Your CodeLocal Forums topic was resolved: " + forumEmailSubjectPart(topic.Title)
	}
	parts := []string{fmt.Sprintf("Topic: %s", topic.Title), fmt.Sprintf("Status: %s", topic.Status)}
	if note := strings.TrimSpace(topic.ResolutionNote); note != "" {
		parts = append(parts, "Resolution note: "+forumEmailSummary(note, 1600))
	}
	if issue := strings.TrimSpace(topic.GitHubIssueURL); issue != "" {
		parts = append(parts, "GitHub issue: "+issue)
	}
	if pr := strings.TrimSpace(topic.GitHubPRURL); pr != "" {
		parts = append(parts, "GitHub PR: "+pr)
	}
	body := strings.Join(parts, "\n")
	text := "CodeLocal updated the status of your forum topic.\n\n" + body + "\n\nOpen: " + topicURL
	htmlBody := forumEmailHTML("Forum topic updated", "CodeLocal updated the status of your forum topic.", body, topicURL)
	return subject, text, htmlBody
}

func (s *Server) enqueueForumEmail(ctx context.Context, input cloud.ForumEmailJobDraft) {
	if err := s.Store.EnqueueForumEmailJob(ctx, input); err != nil {
		slog.Warn("forum email enqueue failed", "event", input.EventType, "topicId", input.TopicID, "commentId", input.CommentID, "audience", input.Audience, "error", err)
	}
}

func (s *Server) queueForumTopicCreatedEmails(ctx context.Context, topic cloud.ForumTopic, actorEmail string) {
	subject, text, htmlBody := forumAdminTopicCreatedEmail(topic)
	actorEmail = strings.ToLower(strings.TrimSpace(actorEmail))
	for _, recipient := range cloud.AdminEmails() {
		if strings.EqualFold(recipient, actorEmail) {
			continue
		}
		s.enqueueForumEmail(ctx, cloud.ForumEmailJobDraft{
			DedupeKey: "forum.topic.created:" + topic.ID + ":" + strings.ToLower(recipient), EventType: "forum.topic.created",
			TopicID: topic.ID, RecipientEmail: recipient, Audience: "admin", Subject: subject, TextBody: text, HTMLBody: htmlBody,
		})
	}
}

func (s *Server) queueForumCommentCreatedEmailsForComment(ctx context.Context, comment cloud.ForumComment) {
	topic, err := s.Store.ForumTopicByID(ctx, comment.TopicID)
	if err != nil {
		slog.Warn("forum comment notification topic lookup failed", "topicId", comment.TopicID, "commentId", comment.ID, "error", err)
		return
	}
	s.queueForumCommentCreatedEmails(ctx, topic, comment)
}

func (s *Server) queueForumCommentCreatedEmails(ctx context.Context, topic cloud.ForumTopic, comment cloud.ForumComment) {
	adminSubject, adminText, adminHTML := forumAdminCommentCreatedEmail(topic, comment)
	actorEmail := strings.ToLower(strings.TrimSpace(comment.AuthorEmail))
	adminRecipients := map[string]struct{}{}
	for _, recipient := range cloud.AdminEmails() {
		normalized := strings.ToLower(strings.TrimSpace(recipient))
		if normalized == "" || normalized == actorEmail {
			continue
		}
		adminRecipients[normalized] = struct{}{}
		s.enqueueForumEmail(ctx, cloud.ForumEmailJobDraft{
			DedupeKey: "forum.comment.created:" + comment.ID + ":admin:" + normalized, EventType: "forum.comment.created",
			TopicID: topic.ID, CommentID: comment.ID, RecipientEmail: normalized, Audience: "admin", Subject: adminSubject, TextBody: adminText, HTMLBody: adminHTML,
		})
	}

	ownerEmail := strings.ToLower(strings.TrimSpace(topic.AuthorEmail))
	if topic.AuthorUserID == comment.AuthorUserID || ownerEmail == "" {
		return
	}
	if _, alreadyAdmin := adminRecipients[ownerEmail]; alreadyAdmin {
		return
	}
	ownerSubject, ownerText, ownerHTML := forumTopicOwnerCommentEmail(topic, comment)
	s.enqueueForumEmail(ctx, cloud.ForumEmailJobDraft{
		DedupeKey: "forum.comment.created:" + comment.ID + ":owner:" + ownerEmail, EventType: "forum.comment.created",
		TopicID: topic.ID, CommentID: comment.ID, RecipientEmail: ownerEmail, Audience: "topic_owner", Subject: ownerSubject, TextBody: ownerText, HTMLBody: ownerHTML,
	})
}

func (s *Server) queueForumStatusChangedEmail(ctx context.Context, previous, current cloud.ForumTopic, actorUserID string) {
	if strings.TrimSpace(previous.Status) == strings.TrimSpace(current.Status) || strings.TrimSpace(current.AuthorEmail) == "" || current.AuthorUserID == strings.TrimSpace(actorUserID) {
		return
	}
	subject, text, htmlBody := forumTopicOwnerStatusEmail(current)
	recipient := strings.ToLower(strings.TrimSpace(current.AuthorEmail))
	s.enqueueForumEmail(ctx, cloud.ForumEmailJobDraft{
		DedupeKey: fmt.Sprintf("forum.topic.status:%s:%d:%s:%s", current.ID, current.UpdatedAt, current.Status, recipient), EventType: "forum.topic.status_changed",
		TopicID: current.ID, RecipientEmail: recipient, Audience: "topic_owner", Subject: subject, TextBody: text, HTMLBody: htmlBody,
	})
}

func (s *Server) forumEmailWorker(ctx context.Context) {
	ticker := time.NewTicker(forumEmailPollInterval)
	defer ticker.Stop()
	for {
		s.processForumEmailJobs(ctx)
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
		}
	}
}

func (s *Server) processForumEmailJobs(ctx context.Context) {
	client, err := mailer.FromEnv()
	if err != nil {
		return
	}
	for batch := 0; batch < 5 && ctx.Err() == nil; batch++ {
		jobs, claimErr := s.Store.ClaimForumEmailJobs(ctx, 20)
		if claimErr != nil {
			slog.Warn("forum email jobs claim failed", "error", claimErr)
			return
		}
		if len(jobs) == 0 {
			return
		}
		for _, job := range jobs {
			sendCtx, cancel := context.WithTimeout(ctx, 15*time.Second)
			sendErr := client.Send(sendCtx, mailer.Message{To: job.RecipientEmail, Subject: job.Subject, Text: job.TextBody, HTML: job.HTMLBody}, "codelocal-"+job.ID)
			cancel()
			if sendErr != nil {
				if retryErr := s.Store.RetryForumEmailJob(ctx, job, sendErr); retryErr != nil {
					slog.Warn("forum email retry state failed", "jobId", job.ID, "error", retryErr)
				}
				slog.Warn("forum email delivery failed", "jobId", job.ID, "event", job.EventType, "audience", job.Audience, "attempt", job.Attempts, "error", sendErr)
				continue
			}
			if markErr := s.Store.MarkForumEmailJobSent(ctx, job.ID); markErr != nil {
				slog.Warn("forum email sent state failed", "jobId", job.ID, "error", markErr)
			}
		}
		if len(jobs) < 20 {
			return
		}
	}
}
