CREATE TABLE "avatar" (
	"user_id" text PRIMARY KEY NOT NULL,
	"blob" "bytea" NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
ALTER TABLE "avatar" ADD CONSTRAINT "avatar_user_id_user_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."user"("id") ON DELETE cascade ON UPDATE no action;