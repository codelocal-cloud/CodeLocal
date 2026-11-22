package cloud

import (
	"context"
	"strings"
	"time"
)

const maxForumEmailAttempts = 5

type ForumEmailJobDraft struct {
	DedupeKey      string
	EventType      string
	TopicID        string
	CommentID      string
	RecipientEmail string
	Audience       string
	Subject        string
	TextBody       string
	HTMLBody       string
}

type ForumEmailJob struct {
	ID             string
	DedupeKey      string
	EventType      string
	TopicID        string
	CommentID      string
	RecipientEmail string
	Audience       string
	Subject        string
	TextBody       string
	HTMLBody       string
	Attempts       int
}

func (s *Store) EnqueueForumEmailJob(ctx context.Context, input ForumEmailJobDraft) error {
	input.DedupeKey = strings.TrimSpace(input.DedupeKey)
	input.EventType = strings.TrimSpace(input.EventType)
	input.TopicID = strings.TrimSpace(input.TopicID)
	input.CommentID = strings.TrimSpace(input.CommentID)
	input.RecipientEmail = normalizeEmail(input.RecipientEmail)
	input.Audience = strings.TrimSpace(input.Audience)
	input.Subject = strings.TrimSpace(input.Subject)
	input.TextBody = strings.TrimSpace(input.TextBody)
	input.HTMLBody = strings.TrimSpace(input.HTMLBody)
	if input.DedupeKey == "" || input.EventType == "" || input.RecipientEmail == "" || input.Subject == "" || input.TextBody == "" || input.HTMLBody == "" {
		return ErrForumInvalid
	}
	if input.Audience != "admin" && input.Audience != "topic_owner" {
		return ErrForumInvalid
	}
	now := time.Now().UnixMilli()
	_, err := s.DB.Exec(ctx, `
INSERT INTO codelocal_forum_email_jobs(
 job_id,dedupe_key,event_type,topic_id,comment_id,recipient_email,audience,subject,text_body,html_body,status,attempts,next_attempt_at,created_at,updated_at
) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'pending',0,$11,$11,$11)
ON CONFLICT (dedupe_key) DO NOTHING`,
		"forumemail_"+RandomHex(12), input.DedupeKey, input.EventType, input.TopicID, input.CommentID,
		input.RecipientEmail, input.Audience, input.Subject, input.TextBody, input.HTMLBody, now,
	)
	return err
}

func (s *Store) ClaimForumEmailJobs(ctx context.Context, limit int) ([]ForumEmailJob, error) {
	if limit < 1 {
		limit = 20
	}
	if limit > 100 {
		limit = 100
	}
	now := time.Now().UnixMilli()
	staleBefore := now - (2 * time.Minute).Milliseconds()
	rows, err := s.DB.Query(ctx, `
WITH picked AS (
 SELECT job_id
 FROM codelocal_forum_email_jobs
 WHERE (status='pending' AND next_attempt_at <= $1)
    OR (status='sending' AND updated_at <= $2)
 ORDER BY next_attempt_at ASC, created_at ASC
 FOR UPDATE SKIP LOCKED
 LIMIT $3
)
UPDATE codelocal_forum_email_jobs AS j
SET status='sending', attempts=j.attempts+1, updated_at=$1
FROM picked
WHERE j.job_id=picked.job_id
RETURNING j.job_id,j.dedupe_key,j.event_type,j.topic_id,j.comment_id,j.recipient_email,j.audience,j.subject,j.text_body,j.html_body,j.attempts`, now, staleBefore, limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	jobs := make([]ForumEmailJob, 0, limit)
	for rows.Next() {
		var job ForumEmailJob
		if err := rows.Scan(&job.ID, &job.DedupeKey, &job.EventType, &job.TopicID, &job.CommentID, &job.RecipientEmail, &job.Audience, &job.Subject, &job.TextBody, &job.HTMLBody, &job.Attempts); err != nil {
			return nil, err
		}
		jobs = append(jobs, job)
	}
	return jobs, rows.Err()
}

func (s *Store) MarkForumEmailJobSent(ctx context.Context, jobID string) error {
	now := time.Now().UnixMilli()
	_, err := s.DB.Exec(ctx, `UPDATE codelocal_forum_email_jobs SET status='sent',sent_at=$2,updated_at=$2,last_error='' WHERE job_id=$1`, strings.TrimSpace(jobID), now)
	return err
}

func (s *Store) RetryForumEmailJob(ctx context.Context, job ForumEmailJob, sendErr error) error {
	now := time.Now().UnixMilli()
	lastError := "email delivery failed"
	if sendErr != nil {
		lastError = strings.TrimSpace(sendErr.Error())
	}
	if len(lastError) > 2000 {
		lastError = lastError[:2000]
	}
	status := "pending"
	attemptForBackoff := job.Attempts
	if attemptForBackoff > 6 {
		attemptForBackoff = 6
	}
	backoff := time.Duration(1<<attemptForBackoff) * time.Minute
	nextAttemptAt := now + backoff.Milliseconds()
	if job.Attempts >= maxForumEmailAttempts {
		status = "failed"
		nextAttemptAt = now
	}
	_, err := s.DB.Exec(ctx, `UPDATE codelocal_forum_email_jobs SET status=$2,next_attempt_at=$3,last_error=$4,updated_at=$5 WHERE job_id=$1`, strings.TrimSpace(job.ID), status, nextAttemptAt, lastError, now)
	return err
}
