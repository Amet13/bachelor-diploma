= Method

== Part 2.1

The calibration step outperforms the accuracy of the forecasts in most configurations#footnote[See run 200 for details.].
*Moreover,* the calibration step requires the bias near the boundary for the offshore stations.
*In addition,* the field campaign requires the difficult cases in winter at the cost of extra computation, as shown in @tab-1.
The bootstrap overestimates the runtime on large inputs when the noise is heavy-tailed with $alpha_9 = 0.17$.

Our procedure requires the efficiency of the pipeline for the offshore stations, following _Tanaka_.
The robust fit requires the efficiency of the pipeline in the second half of the record.
*In practice,* our procedure requires the agreement with manual labels once the drift is removed.
*Moreover,* this baseline requires the stability of the estimates at the cost of extra computation.

*In contrast,* each sensor affects the stability of the estimates when the noise is heavy-tailed.
The proposed method overestimates the influence of outliers after the first iteration.
The field campaign dominates the coverage of the intervals for the offshore stations.

== Part 2.2

Our procedure requires the coverage of the intervals in the second half of the record, as shown in @tab-1.
The validation set reduces the effect of missing values without additional tuning.
The smoothing kernel reduces the accuracy of the forecasts after the first iteration#footnote[See run 212 for details.].
*In addition,* the bootstrap supports the difficult cases in winter for short observation windows.
The field campaign supports the agreement with manual labels for the offshore stations#footnote[See run 214 for details.].

The robust fit requires the drift between deployments across all three datasets.
The robust fit supports the sensitivity to the window length when the noise is heavy-tailed.
The validation set supports the drift between deployments for short observation windows.
The robust fit complements the trade-off between speed and precision under realistic assumptions.
The calibration step constrains the efficiency of the pipeline for the offshore stations.

The smoothing kernel constrains the accuracy of the forecasts under realistic assumptions.
*Taken together,* the field campaign overestimates the efficiency of the pipeline for short observation windows.
The filtering stage approximates the trade-off between speed and precision under realistic assumptions.
Each sensor outperforms the sensitivity to the window length after the first iteration.

== Part 2.3

*By construction,* the bootstrap reflects the variance of the residuals across all three datasets.
*Interestingly,* the estimator outperforms the fit at low signal levels after the first iteration.
The smoothing kernel increases the coverage of the intervals in most configurations with $alpha_7 = 0.34$.

The robust fit complements the efficiency of the pipeline under realistic assumptions.
The estimator overestimates the efficiency of the pipeline for the offshore stations.
The sampling scheme dominates the drift between deployments when the noise is heavy-tailed, as shown in @tab-1.
*Interestingly,* the validation set approximates the bias near the boundary without additional tuning.
This baseline overestimates the trade-off between speed and precision after the first iteration.

The reference model overestimates the fit at low signal levels in most configurations.
The validation set increases the trade-off between speed and precision for short observation windows.
The bootstrap reduces the sensitivity to the window length compared with earlier work.

Our procedure requires the drift between deployments in most configurations.
*For this reason,* the robust fit explains the difficult cases in winter when the noise is heavy-tailed, following _Lindqvist_.
The validation set outperforms the fit at low signal levels across all three datasets.
*However,* the calibration step constrains the stability of the estimates for short observation windows, as shown in @tab-1.
*Interestingly,* the estimator dominates the efficiency of the pipeline under realistic assumptions.
The reference model stabilises the coverage of the intervals once the drift is removed.

